use std::collections::HashMap;
use std::process::Command;

use anyhow::{anyhow, Result};
use chrono::{DateTime, Local, NaiveDateTime, Utc};
use clap::{App, Arg};
use colored::Colorize;
use extend::ext;
use itertools::Itertools;
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::blocking::{Client, ClientBuilder};
use reqwest::Proxy;
use scraper::{Html, Selector};
use serde::Deserialize;

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
(KHTML, like Gecko) Chrome/91.0.4472.164 Safari/537.36";

static ID_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"https://www\.cbc\.ca/player/play/([[:digit:]]+)"#).unwrap());
static SEQ_SELECTOR: Lazy<Selector> = Lazy::new(|| Selector::parse("seq > video").unwrap());
static STATE_SELECTOR: Lazy<Selector> =
    Lazy::new(|| Selector::parse("script#initialStateDom").unwrap());

fn main() -> Result<()> {
    let matches = App::new("cbc-sl")
        .arg(
            Arg::new("proxy")
                .takes_value(true)
                .short('p')
                .long("proxy")
                .about("SOCKS5 proxy to use"),
        )
        .arg(
            Arg::new("URL")
                .required_unless_present_any(&["list", "replays"])
                .about("CBC.ca URL or ID")
                .validator(probably_cbc),
        )
        .arg(Arg::new("run").short('r').long("run").about("Run streamlink"))
        .arg(
            Arg::new("list")
                .short('l')
                .long("list")
                .about("List available Olympics streams")
                .conflicts_with_all(&["URL", "run", "replays"]),
        )
        .arg(
            Arg::new("replays")
                .short('a')
                .long("replays")
                .about("List available Olympics replays. Limited to the last 24 hours")
                .conflicts_with_all(&["URL", "run", "list"]),
        )
        .get_matches();
    let mut cb = ClientBuilder::new();
    if let Some(proxy) = matches.value_of("proxy") {
        cb = cb.proxy(Proxy::all(proxy_url(proxy))?);
    }

    let client = cb.user_agent(USER_AGENT).build()?;
    if matches.is_present("list") {
        return list(&client, true);
    }
    if matches.is_present("replays") {
        return list(&client, false);
    }
    let id = parse_cbc_id(matches.value_of("URL").unwrap())?;
    let target = format!("https://www.cbc.ca/bistro/order?mediaId={}&limit=10&sort=dateAired", id);
    let bistro: Bistro = client.get(target).send()?.error_for_status()?.json()?;
    let desc: &AssetDescriptor = bistro // intellij infers wrong type
        .items
        .get(0)
        .ok_or_else(|| anyhow!("missing item"))?
        .asset_descriptors
        .iter()
        .find(|id| id.loader == "PlatformLoader")
        .ok_or_else(|| anyhow!("couldn't find PlatformLoader"))?;
    let smil = client.get(&desc.key).send()?.error_for_status()?.text()?;
    let doc = Html::parse_document(&smil);
    let video = doc.select(&SEQ_SELECTOR).next().ok_or_else(|| anyhow!("missing seq"))?;
    let src = video.value().attr("src").ok_or_else(|| anyhow!("missing src"))?;
    if !matches.is_present("run") {
        println!("{}", src);
    } else {
        let stat = if let Some(proxy) = matches.value_of("proxy").map(|p| proxy_url(p)) {
            Command::new("streamlink")
                .arg("--http-proxy") // also proxies https
                .arg(&proxy)
                .arg(src)
                .arg("best")
                .status()?
        } else {
            Command::new("streamlink").arg(src).arg("best").status()?
        };
        if !stat.success() {
            return if stat.code().is_some() {
                Err(anyhow!("streamlink exit code: {}", stat.code().unwrap()))
            } else {
                Err(anyhow!("streamlink exited unexpectedly"))
            };
        }
    }
    Ok(())
}

fn list(client: &Client, live: bool) -> Result<()> {
    #[cfg(windows)]
    colored::control::set_virtual_terminal(true).unwrap();
    let url = if live {
        "https://www.cbc.ca/player/sports/olympics/live"
    } else {
        "https://www.cbc.ca/player/sports/olympics/replays"
    };
    let page = client.get(url).send()?.error_for_status()?.text()?;
    let doc = Html::parse_document(&page);
    let script = doc
        .select(&STATE_SELECTOR)
        .next()
        .ok_or_else(|| anyhow!("couldn't find state"))?
        .text()
        .join("")
        .substring_from("{")
        .substring_to_last("}")
        .to_owned();
    let live: LiveState = serde_json::from_str(&script)?;
    for video in live.video.live_clips.on_now.items.iter() {
        println!("{}", video.to_human(VidState::Started));
    }
    for video in live.video.live_clips.upcoming.items.iter() {
        println!("{}", video.to_human(VidState::Upcoming));
    }
    for (_category, clips) in live.video.clips_by_category.iter() {
        for video in clips.items.iter() {
            println!("{}", video.to_human(VidState::Replay));
        }
    }
    Ok(())
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum VidState {
    Started,
    Upcoming,
    Replay,
}

#[ext]
impl Item {
    fn to_human(&self, state: VidState) -> String {
        let air = NaiveDateTime::from_timestamp(self.air_date / 1000, 0);
        let air: DateTime<Utc> = DateTime::from_utc(air, Utc);
        let now = Local::now();
        let air = air.with_timezone(&Local);
        let text =
            if now.date() == air.date() { air.format("%H:%M") } else { air.format("%b %d %H:%M") };
        let note = match state {
            VidState::Started => format!("({} @ {}) ", "STARTED ", text),
            VidState::Upcoming => format!("({} @ {}) ", "UPCOMING".dimmed(), text),
            VidState::Replay => format!("({}) ", text),
        };
        format!("{} - {}{}", self.id, note, self.title)
    }
}

fn parse_cbc_id(input: &str) -> Result<u64> {
    Ok(if input.is_numeric() {
        input.parse()?
    } else {
        ID_REGEX.captures(input).unwrap().get(1).unwrap().as_str().parse()?
    })
}

#[ext]
impl str {
    fn is_numeric(&self) -> bool {
        !self.is_empty() && self.chars().all(|c| c.is_ascii_digit())
    }
    fn substring_to_last(&self, pat: &str) -> &str {
        if self.is_empty() || pat.is_empty() {
            return self;
        }
        match self.rfind(pat) {
            None => self,
            Some(index) => &self[..index + 1],
        }
    }
    fn substring_from(&self, pat: &str) -> &str {
        if self.is_empty() || pat.is_empty() {
            return self;
        }
        match self.find(pat) {
            None => self,
            Some(index) => &self[index..],
        }
    }
}

/// Rewrites a proxy specification like `1.2.3.4:8080` to have `socks5h://` in front.
fn proxy_url(spec: &str) -> String {
    // TODO: Support and rewrite URLs that already have socks5(h)://
    // TODO: Do standard HTTP proxies work with CBC? Haven't bothered to test
    format!("socks5h://{}", spec)
}

/// Returns OK if the input is either numeric (ID) or a full CBC URL.
fn probably_cbc(input: &str) -> std::result::Result<(), String> {
    if input.is_numeric() || ID_REGEX.is_match(input) {
        Ok(())
    } else {
        Err("invalid url".into())
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct Bistro {
    pub(crate) items: Vec<OrderItem>,
    pub(crate) errors: Vec<Option<serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OrderItem {
    pub(crate) title: String,
    pub(crate) description: String,
    #[serde(rename = "showName")]
    pub(crate) show_name: String,
    // pub(crate) categories: Vec<Category>,
    pub(crate) thumbnail: String,
    // #[serde(rename = "hostImage")]
    // pub(crate) host_image: Option<serde_json::Value>,
    // pub(crate) chapters: Vec<Chapter>,
    // pub(crate) duration: i64,
    // #[serde(rename = "airDate")]
    // pub(crate) air_date: i64,
    // #[serde(rename = "addedDate")]
    // pub(crate) added_date: i64,
    #[serde(rename = "contentArea")]
    pub(crate) content_area: String,
    pub(crate) season: String,
    pub(crate) episode: String,
    #[serde(rename = "type")]
    pub(crate) item_type: String,
    pub(crate) region: String,
    pub(crate) sport: String,
    pub(crate) genre: String,
    pub(crate) captions: bool,
    pub(crate) token: String,
    #[serde(rename = "pageUrl")]
    pub(crate) page_url: String,
    #[serde(rename = "adUrl")]
    pub(crate) ad_url: String,
    #[serde(rename = "adOrder")]
    pub(crate) ad_order: String,
    #[serde(rename = "isAudio")]
    pub(crate) is_audio: bool,
    #[serde(rename = "isVideo")]
    pub(crate) is_video: bool,
    #[serde(rename = "isLive")]
    pub(crate) is_live: bool,
    #[serde(rename = "isOnDemand")]
    pub(crate) is_on_demand: bool,
    #[serde(rename = "isDRM")]
    pub(crate) is_drm: bool,
    #[serde(rename = "isBlocked")]
    pub(crate) is_blocked: bool,
    #[serde(rename = "isDAI")]
    pub(crate) is_dai: bool,
    // #[serde(rename = "embeddedVia")]
    // pub(crate) embedded_via: bool,
    // pub(crate) keywords: String,
    // #[serde(rename = "brandedSponsorName")]
    // pub(crate) branded_sponsor_name: String,
    // #[serde(rename = "originalDepartment")]
    // pub(crate) original_department: String,
    // #[serde(rename = "mediaPublisherName")]
    // pub(crate) media_publisher_name: String,
    // #[serde(rename = "mediaPublisherType")]
    // pub(crate) media_publisher_type: String,
    // #[serde(rename = "adCategoryExclusion")]
    // pub(crate) ad_category_exclusion: String,
    // #[serde(rename = "excludeFromRecommendations")]
    // pub(crate) exclude_from_recommendations: bool,
    pub(crate) id: String,
    #[serde(rename = "idType")]
    pub(crate) id_type: String,
    #[serde(rename = "assetDescriptors")]
    pub(crate) asset_descriptors: Vec<AssetDescriptor>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AssetDescriptor {
    pub(crate) loader: String,
    pub(crate) key: String,
    #[serde(rename = "mimeType")]
    pub(crate) mime_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Category {
    pub(crate) name: String,
    pub(crate) scheme: String,
    pub(crate) label: String,
}

// #[derive(Debug, Deserialize)]
// pub struct Chapter {
//     #[serde(rename = "startTime")]
//     pub(crate) start_time: i64,
//     pub(crate) name: String,
// }

// Listing structs

#[derive(Debug, Deserialize)]
pub(crate) struct LiveState {
    pub(crate) video: Video,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Video {
    // #[serde(rename = "currentClip")]
    // pub(crate) current_clip: CurrentClip,
    // pub(crate) recommendations: AcrossBaseCategoriesClips,
    // #[serde(rename = "acrossBaseCategoriesClips")]
    // pub(crate) across_base_categories_clips: AcrossBaseCategoriesClips,
    #[serde(rename = "liveClips")]
    pub(crate) live_clips: LiveClips,
    // pub(crate) category: VideoCategory,
    // #[serde(rename = "discoverCategories")]
    // pub(crate) discover_categories: Vec<Option<serde_json::Value>>,
    // #[serde(rename = "featuredCategories")]
    // pub(crate) featured_categories: Vec<Option<serde_json::Value>>,
    #[serde(rename = "clipsByCategory")]
    pub(crate) clips_by_category: HashMap<String, Clips>,
    // #[serde(rename = "trendingClips")]
    // pub(crate) trending_clips: AcrossBaseCategoriesClips,
    // #[serde(rename = "childCategories")]
    // pub(crate) child_categories: ChildCategories,
    // #[serde(rename = "localClips")]
    // pub(crate) local_clips: AcrossBaseCategoriesClips,
    // #[serde(rename = "curatedPlaylist")]
    // pub(crate) curated_playlist: AcrossBaseCategoriesClips,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Clips {
    pub(crate) items: Vec<Item>,
    // #[serde(rename = "isLoaded")]
    // pub(crate) is_loaded: bool, // not universal
}

#[derive(Debug, Deserialize)]
pub(crate) struct Item {
    pub(crate) id: i64,
    pub(crate) source: String,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) thumbnail: String,
    pub(crate) duration: i64,
    #[serde(rename = "airDate")]
    pub(crate) air_date: i64,
    #[serde(rename = "contentArea")]
    pub(crate) content_area: String,
    pub(crate) sport: Option<String>,
    #[serde(rename = "showName")]
    pub(crate) show_name: String,
    // pub(crate) captions: Captions,
    pub(crate) categories: Vec<CategoryElement>,
    #[serde(rename = "isLive")]
    pub(crate) is_live: bool,
    #[serde(rename = "isVideo")]
    pub(crate) is_video: bool,
}

// #[derive(Debug, Deserialize)]
// pub(crate) struct Captions {
//     pub(crate) src: Option<serde_json::Value>,
//     pub(crate) lang: String,
// }

#[derive(Debug, Deserialize)]
pub(crate) struct CategoryElement {
    pub(crate) name: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct VideoCategory {
    #[serde(rename = "fullTitle")]
    pub(crate) full_title: String,
    pub(crate) title: String,
    pub(crate) path: String,
}

// #[derive(Debug, Deserialize)]
// pub struct ChildCategories {
//     pub(crate) items: Vec<Option<serde_json::Value>>,
// }

// #[derive(Debug, Deserialize)]
// pub struct ClipsByCategory {
//     #[serde(rename = "Sports/Olympics/Summer/Live")]
//     pub(crate) sports_olympics_summer_live: ChildCategories,
// }

// #[derive(Debug, Deserialize)]
// pub struct CurrentClip {
// }

#[derive(Debug, Deserialize)]
pub(crate) struct LiveClips {
    pub(crate) custom: Clips,
    #[serde(rename = "onNow")]
    pub(crate) on_now: Clips,
    pub(crate) upcoming: Clips,
}
