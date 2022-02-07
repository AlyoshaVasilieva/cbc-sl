use std::path::PathBuf;
use std::process::Command;

use anyhow::{anyhow, ensure, Result};
use chrono::{DateTime, Local, NaiveDateTime, Utc};
use clap::Parser;
use extend::ext;
use hls_m3u8::tags::VariantStream;
use hls_m3u8::MasterPlaylist;
use once_cell::sync::Lazy;
use owo_colors::{OwoColorize, Stream::Stdout};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use ureq::{Agent, AgentBuilder, Proxy};
use url::Url;

#[cfg(windows)]
mod wincolors;

// pretend to be a real browser
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
(KHTML, like Gecko) Chrome/98.0.4758.80 Safari/537.36 Edg/98.0.1108.43";

static ID_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"https://www\.cbc\.ca/player/play/([[:digit:]]+)"#).unwrap());

#[derive(Debug, Parser)]
#[clap(version)]
#[clap(about)]
struct Args {
    /// Proxy to use (if you aren't in Canada). If no scheme is set, defaults to socks5
    #[clap(short = 'p', long = "proxy")]
    proxy: Option<String>,
    /// Don't run streamlink, just print the stream URL
    #[clap(short = 'n', long = "no-run", conflicts_with_all(&["list", "replays"]))]
    no_run: bool,
    /// List available Olympics streams
    #[clap(short = 'l', long = "list", conflicts_with_all(&["url", "replays"]))]
    list: bool,
    /// List available Olympics replays (at most 24 are shown)
    #[clap(short = 'a', long = "replays", conflicts_with_all(&["url", "list"]))]
    replays: bool,
    /// Streamlink log level
    #[clap(long = "loglevel", possible_values(["none", "error", "warning", "info", "debug", "trace"]), default_value = "info")]
    loglevel: String,
    /// Trust Streamlink to handle the master playlist. May require streamlink version < 2.3.0 or
    /// > 3.1.1
    #[clap(short = 'T', long = "trust-streamlink")]
    trust: bool,
    /// Stream quality to request. Currently requires --trust-streamlink
    #[clap(short = 'q', long = "quality", default_value = "best")]
    quality: String,
    /// Streamlink bin name or path
    #[clap(short = 'S', long = "streamlink", default_value = "streamlink")]
    streamlink: PathBuf,
    /// Show full URLs when listing events
    #[clap(short = 'f', long = "full-urls")]
    full_urls: bool,
    /// CBC.ca URL or ID
    #[clap(validator(probably_cbc), required_unless_present_any(["list", "replays"]))]
    url: Option<String>,
}

fn get_live_and_upcoming(agent: &Agent) -> Result<LiveResponse> {
    /// Hit "Show More" and look at the network monitor to get this
    const LIVE_QUERY: &str = "query clipsFromCategory($categoryName: String, $page: Int, \
    $pageSize: Int, $onNowBeforeDate: Float, $onNowAfterDate: Float) {\n        \
    mpxItems (categoryName: $categoryName, pageSize: $pageSize, page: $page, afterDate: \
    $onNowAfterDate, beforeDate: $onNowBeforeDate, sortBy: \"pubDate\") {\n            \
    ...mediaItemBaseCard\n        }\n    } fragment mediaItemBaseCard on MediaItem {\n    \
    ...mediaItemBase\n    description\n    sport\n    showName\n    captions {\n        \
    src\n        lang\n    }\n} fragment mediaItemBase on MediaItem {\n    id\n    source\n    \
    title\n    thumbnail\n    airDate\n    duration\n    contentArea\n    categories {\n        \
    name\n    }\n    isLive\n    isVideo\n}";

    let now = Utc::now();
    let start = now - chrono::Duration::hours(14);
    let query = json!({
        "query": LIVE_QUERY,
        "variables": {
            "categoryName": "Sports/Olympics/Winter/Live",
            // "onNowBeforeDate": end.timestamp(), // give us the full schedule
            "onNowAfterDate": start.timestamp(),
            "pageSize": 24, // normally 4
            "page": 1
        }
    });
    Ok(agent.post("https://www.cbc.ca/graphql").send_json(query)?.into_json()?)
}

fn get_replays(agent: &Agent) -> Result<LiveResponse> {
    /// Built from the live query, with sorting removed so it gives proper order (recent first).
    /// CBC's site uses a much different query for grabbing replays but this works and allows
    /// reuse of deserialization.
    const VOD_QUERY: &str = "query clipsFromCategory($categoryName: String, $page: Int, \
    $pageSize: Int, $onNowBeforeDate: Float, $onNowAfterDate: Float) {\n        \
    mpxItems (categoryName: $categoryName, pageSize: $pageSize, page: $page, afterDate: \
    $onNowAfterDate, beforeDate: $onNowBeforeDate) {\n            \
    ...mediaItemBaseCard\n        }\n    } fragment mediaItemBaseCard on MediaItem {\n    \
    ...mediaItemBase\n    description\n    sport\n    showName\n    captions {\n        \
    src\n        lang\n    }\n} fragment mediaItemBase on MediaItem {\n    id\n    source\n    \
    title\n    thumbnail\n    airDate\n    duration\n    contentArea\n    categories {\n        \
    name\n    }\n    isLive\n    isVideo\n}";

    let query = json!({
        "query": VOD_QUERY,
        "variables": {
            "categoryName": "Sports/Olympics/Winter/Replays",
            // "onNowBeforeDate": end.timestamp(),
            // "onNowAfterDate": start.timestamp(),
            "pageSize": 24,
            "page": 1
        }
    });
    Ok(agent.post("https://www.cbc.ca/graphql").send_json(query)?.into_json()?)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GqlQuery {
    pub(crate) query: String,
    pub(crate) variables: Variables,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Variables {
    #[serde(rename = "categoryName")]
    pub(crate) category_name: String,
    #[serde(rename = "onNowBeforeDate")]
    pub(crate) on_now_before_date: i64,
    #[serde(rename = "onNowAfterDate")]
    pub(crate) on_now_after_date: i64,
    #[serde(rename = "pageSize")]
    pub(crate) page_size: i64,
    pub(crate) page: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LiveResponse {
    pub(crate) data: LiveData,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LiveData {
    #[serde(rename = "mpxItems")]
    pub(crate) mpx_items: Vec<MpxItem>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MpxItem {
    pub(crate) id: i64,
    // pub(crate) source: String,
    pub(crate) title: String,
    // pub(crate) thumbnail: String,
    #[serde(rename = "airDate")]
    pub(crate) air_date: i64,
    // pub(crate) duration: i64,
    // #[serde(rename = "contentArea")]
    // pub(crate) content_area: String,
    // pub(crate) categories: Vec<Category>,
    #[serde(rename = "isLive")]
    pub(crate) is_live: bool,
    #[serde(rename = "isVideo")]
    pub(crate) is_video: bool,
    // pub(crate) description: String,
    // pub(crate) sport: String, // included in title
    // #[serde(rename = "showName")]
    // pub(crate) show_name: String,
    // pub(crate) captions: Captions, // not available
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum VidState {
    LiveOrUpcoming,
    Replay,
}

impl MpxItem {
    fn to_human(&self, state: VidState, full_urls: bool) -> String {
        let air = self.air_date();
        let now = Local::now();
        let text =
            if now.date() == air.date() { air.format("%H:%M") } else { air.format("%b %d %H:%M") };
        let is_live = Utc::now().timestamp_millis() >= self.air_date;
        let note = match state {
            VidState::LiveOrUpcoming => match is_live {
                true => format!(
                    "({} @ {}) ",
                    "STARTED ".if_supports_color(Stdout, |text| text.bright_white().on_black()),
                    text
                ),
                false => format!(
                    "({} @ {}) ",
                    "UPCOMING".if_supports_color(Stdout, |text| text.white().on_black()),
                    text
                ),
            },
            VidState::Replay => format!("({}) ", text),
        };
        // bright white: white
        // white: light gray
        let prefix = if full_urls { "https://www.cbc.ca/player/play/" } else { "" };
        format!("{}{} - {}{}", prefix, self.id, note, self.title)
    }

    fn air_date(&self) -> DateTime<Local> {
        let air = NaiveDateTime::from_timestamp(self.air_date / 1000, 0);
        let air: DateTime<Utc> = DateTime::from_utc(air, Utc);
        air.with_timezone(&Local)
    }
}

// #[derive(Debug, Serialize, Deserialize)]
// pub struct Captions {
//     pub(crate) src: Option<serde_json::Value>,
//     pub(crate) lang: String,
// }

// #[derive(Debug, Serialize, Deserialize)]
// pub struct Category {
//     pub(crate) name: String,
// }

fn main() -> Result<()> {
    let args = Args::parse();
    #[cfg(windows)]
    let _ = wincolors::enable_colors();
    let mut ab = AgentBuilder::new().user_agent(USER_AGENT);
    if let Some(proxy) = args.proxy.as_deref() {
        ab = ab.proxy(Proxy::new(proxy_url_ureq(proxy))?);
    }
    let agent = ab.build();
    if args.list {
        for item in get_live_and_upcoming(&agent)?.data.mpx_items {
            println!("{}", item.to_human(VidState::LiveOrUpcoming, args.full_urls));
        }
        return Ok(());
    }
    if args.replays {
        for item in get_replays(&agent)?.data.mpx_items {
            println!("{}", item.to_human(VidState::Replay, args.full_urls));
        }
        return Ok(());
    }

    let id = parse_cbc_id(&args.url.unwrap())?;
    let target = format!("https://www.cbc.ca/bistro/order?mediaId={}&limit=10&sort=dateAired", id);
    let bistro: Bistro = agent.get(&target).call()?.into_json()?;
    let desc = bistro
        .items
        .get(0)
        .ok_or_else(|| anyhow!("missing item"))?
        .asset_descriptors
        .iter()
        .find(|id| id.loader == "PlatformLoader")
        .ok_or_else(|| anyhow!("couldn't find PlatformLoader - are you Canadian?"))?;
    let smil = agent.get(&desc.key).call()?.into_string()?;
    let smil: Smil = quick_xml::de::from_str(&smil)?;
    let master_playlist = &smil.body.seq.video.first().ok_or_else(|| anyhow!("missing video"))?.src;
    let stream = if args.trust {
        master_playlist.to_owned()
    } else {
        let playlist = agent.get(master_playlist).call()?.into_string()?;
        get_best_stream(master_playlist, &playlist)?
    };
    if args.no_run {
        println!("{}", stream);
    } else {
        let sl = args.streamlink;
        let stat = if let Some(proxy) = args.proxy.map(|p| proxy_url_streamlink(&p)) {
            Command::new(sl)
                .arg("--loglevel")
                .arg(&args.loglevel)
                .arg("--http-header")
                .arg(format!("User-Agent={USER_AGENT}"))
                .arg("--http-proxy") // also proxies https
                .arg(&proxy)
                .arg(stream)
                .arg(args.quality)
                .status()?
        } else {
            Command::new(sl)
                .arg("--loglevel")
                .arg(&args.loglevel)
                .arg("--http-header")
                .arg(format!("User-Agent={USER_AGENT}"))
                .arg(stream)
                .arg(args.quality)
                .status()?
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

/// Given the URL of the master playlist, and its contents, get the highest-bandwidth stream
/// and build an absolute URL to it.
///
/// Workaround for https://github.com/streamlink/streamlink/issues/4329
fn get_best_stream(url: &str, mp: &str) -> Result<String> {
    let best = parse_master_playlist(mp)?;
    let mut url = Url::parse(url)?;
    url.set_query(None);
    url.path_segments_mut().unwrap().pop();
    Ok(format!("{}/{}", url.as_str(), best))
}

/// Parse a master playlist, return the URI of the stream with the highest bandwidth.
fn parse_master_playlist(input: &str) -> Result<String> {
    let mp = MasterPlaylist::try_from(input)?;
    let mut variant = mp.variant_streams;
    ensure!(!variant.is_empty(), "no streams found");
    variant.sort_by_key(|v| v.bandwidth());
    variant.reverse();
    let best = variant.first().unwrap();
    Ok(best.uri())
}

#[ext]
impl VariantStream<'_> {
    fn uri(&self) -> String {
        match self {
            Self::ExtXStreamInf { uri, .. } | Self::ExtXIFrame { uri, .. } => uri.to_string(),
        }
    }
}

#[ext]
impl str {
    fn is_numeric(&self) -> bool {
        !self.is_empty() && self.chars().all(|c| c.is_ascii_digit())
    }
    // good chance these do something wrong, since I wrote them in a minute and never tested them
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

/// Rewrites proxy specifications:
/// * SOCKS4 is changed to specify remote DNS
/// * SOCKS5 strips the `h` if present, since ureq always does remote DNS and can't handle `SOCKS5H`
/// * Missing scheme becomes` socks5://`
fn proxy_url_ureq(spec: &str) -> String {
    // We may need remote DNS to avoid geoblocking (ureq always does remote DNS with SOCKS5)
    let mut spec = spec.replacen("socks5h:", "socks5:", 1).replacen("socks4:", "socks4a:", 1);
    if !spec.contains("://") {
        spec = format!("socks5://{}", spec);
    }
    spec
}

/// Rewrites proxy specifications:
/// * SOCKS4/5 is changed to specify remote DNS
/// * Missing scheme becomes `socks5h://`
fn proxy_url_streamlink(spec: &str) -> String {
    let mut spec = spec.replacen("socks5:", "socks5h:", 1).replacen("socks4:", "socks4a:", 1);
    if !spec.contains("://") {
        spec = format!("socks5h://{}", spec);
    }
    spec
}

/// Returns OK if the input is either numeric (ID) or a full CBC URL.
fn probably_cbc(input: &str) -> std::result::Result<(), String> {
    if input.is_numeric() || ID_REGEX.is_match(input) {
        Ok(())
    } else {
        Err("invalid url".into())
    }
}

fn parse_cbc_id(input: &str) -> Result<u64> {
    Ok(if input.is_numeric() {
        input.parse()?
    } else {
        ID_REGEX.captures(input).unwrap().get(1).unwrap().as_str().parse()?
    })
}

#[derive(Debug, Deserialize)]
pub(crate) struct Bistro {
    pub(crate) items: Vec<OrderItem>,
    // pub(crate) errors: Vec<Option<serde_json::Value>>, // never seen this filled
}

#[derive(Debug, Deserialize)]
pub(crate) struct OrderItem {
    // pub(crate) title: String,
    // pub(crate) description: String,
    // #[serde(rename = "showName")]
    // pub(crate) show_name: String,
    // pub(crate) categories: Vec<Category>,
    // pub(crate) thumbnail: String,
    // #[serde(rename = "hostImage")]
    // pub(crate) host_image: Option<serde_json::Value>,
    // pub(crate) chapters: Vec<Chapter>,
    // pub(crate) duration: i64,
    // #[serde(rename = "airDate")]
    // pub(crate) air_date: i64,
    // #[serde(rename = "addedDate")]
    // pub(crate) added_date: i64,
    // #[serde(rename = "contentArea")]
    // pub(crate) content_area: String,
    // pub(crate) season: String,
    // pub(crate) episode: String,
    // #[serde(rename = "type")]
    // pub(crate) item_type: String,
    // pub(crate) region: String,
    // pub(crate) sport: String,
    // pub(crate) genre: String,
    // pub(crate) captions: bool,
    // pub(crate) token: String,
    // #[serde(rename = "pageUrl")]
    // pub(crate) page_url: String,
    // #[serde(rename = "adUrl")]
    // pub(crate) ad_url: String,
    // #[serde(rename = "adOrder")]
    // pub(crate) ad_order: String,
    // #[serde(rename = "isAudio")]
    // pub(crate) is_audio: bool,
    // #[serde(rename = "isVideo")]
    // pub(crate) is_video: bool,
    // #[serde(rename = "isLive")]
    // pub(crate) is_live: bool,
    // #[serde(rename = "isOnDemand")]
    // pub(crate) is_on_demand: bool,
    // #[serde(rename = "isDRM")]
    // pub(crate) is_drm: bool,
    // #[serde(rename = "isBlocked")]
    // pub(crate) is_blocked: bool,
    // #[serde(rename = "isDAI")]
    // pub(crate) is_dai: bool,
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
    // pub(crate) id: String,
    // #[serde(rename = "idType")]
    // pub(crate) id_type: String,
    #[serde(rename = "assetDescriptors")]
    pub(crate) asset_descriptors: Vec<AssetDescriptor>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AssetDescriptor {
    pub(crate) loader: String,
    pub(crate) key: String,
    // #[serde(rename = "mimeType")]
    // pub(crate) mime_type: Option<String>,
}

// #[derive(Debug, Deserialize)]
// pub(crate) struct Category {
//     pub(crate) name: String,
//     pub(crate) scheme: String,
//     pub(crate) label: String,
// }

// #[derive(Debug, Deserialize)]
// pub struct Chapter {
//     #[serde(rename = "startTime")]
//     pub(crate) start_time: i64,
//     pub(crate) name: String,
// }

#[derive(Debug, Deserialize)]
pub(crate) struct Smil {
    body: SmilBody,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SmilBody {
    seq: SmilSeq,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SmilSeq {
    video: Vec<SmilVideo>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SmilVideo {
    src: String,
}
