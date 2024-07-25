use anyhow::Result;
use jiff::{tz::TimeZone, Span, Timestamp, Zoned};
use owo_colors::{OwoColorize, Stream::Stdout};
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GqlResponse {
    pub(crate) data: Data,
    // pub(crate) extensions: Extensions,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Data {
    pub(crate) all_content_items: AllContentItems,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AllContentItems {
    pub(crate) nodes: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub(crate) id: i64,
    pub(crate) url: String,
    pub(crate) title: String,
    // pub(crate) section_list: Vec<Option<serde_json::Value>>,
    // pub(crate) section_labels: Vec<Option<serde_json::Value>>,
    // pub(crate) related_links: Vec<Option<serde_json::Value>>,
    // pub(crate) deck: Option<serde_json::Value>,
    // pub(crate) description: String,
    pub(crate) flag: Flag,
    // pub(crate) image_large: String,
    // pub(crate) image: HashMap<String, Image>,
    // pub(crate) source: String,
    // pub(crate) source_id: String,
    pub(crate) published_at: String,
    pub(crate) updated_at: String,
    // pub(crate) sponsor: Option<serde_json::Value>,
    #[serde(rename = "type")]
    // pub(crate) node_type: Type,
    pub(crate) node_type: String,
    // pub(crate) show_name: Option<String>,
    // pub(crate) authors: Vec<Option<serde_json::Value>>,
    // pub(crate) comments_enabled: bool,
    // pub(crate) contextual_headlines: Vec<Option<serde_json::Value>>,
    // pub(crate) media_id: Option<String>,
    pub(crate) media: Media,
    // pub(crate) headline_data: Option<serde_json::Value>,
    // pub(crate) components: Option<serde_json::Value>,
    // pub(crate) categories: Vec<Category>,
}

impl Node {
    pub(crate) fn proper_id(&self) -> &str {
        self.url.split('/').last().unwrap()
    }

    pub(crate) fn to_human(&self, full_urls: bool) -> Result<String> {
        let now = Zoned::now();
        let date = self.date()?;
        let same_day = now.date() == date.date();
        let fmt = if same_day { "%H:%M" } else { "%b %d %H:%M" };
        let date_time = self.date()?.strftime(fmt);

        // live or upcoming
        let lu = matches!(self.flag, Flag::Live);
        let note = if lu {
            match self.is_live()? {
                true => format!(
                    "({} @ {}) ",
                    "STARTED ".if_supports_color(Stdout, |text| text.bright_white().on_black()),
                    date_time
                ),
                false => format!(
                    "({} @ {}) ",
                    "UPCOMING".if_supports_color(Stdout, |text| text.white().on_black()),
                    date_time
                ),
            }
        } else {
            format!("({}) ", date_time)
        };
        let prefix = if full_urls { "https://www.cbc.ca/player/play/video/" } else { "" };
        Ok(format!("{prefix}{} - {note}{}", self.proper_id(), self.title))
    }

    pub(crate) fn timestamp(&self) -> Result<Timestamp> {
        Ok(Timestamp::from_millisecond(self.published_at.parse()?)?)
    }

    pub(crate) fn date(&self) -> Result<Zoned> {
        Ok(Zoned::new(self.timestamp()?, TimeZone::system()))
    }

    pub(crate) fn is_live(&self) -> Result<bool> {
        let start = self.timestamp()?;
        let duration = self.media.duration.round() as i64;
        let duration = Span::new().seconds(duration);
        let end = start.checked_add(duration)?;
        let now = Timestamp::now();
        Ok(start <= now && now <= end)
    }
}

// #[derive(Debug, Clone, PartialEq, Deserialize)]
// pub struct Category {
//     pub(crate) name: String,
//     pub(crate) slug: String,
//     pub(crate) path: String,
// }

#[derive(Copy, Debug, Clone, PartialEq, Deserialize)]
pub enum Flag {
    Live,
    Video,
}

// #[derive(Debug, Clone, PartialEq, Deserialize)]
// pub struct Image {
//     pub(crate) w: i64,
//     pub(crate) fileurl: String,
// }

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Media {
    pub(crate) duration: f64,
    pub(crate) has_captions: bool,
    pub(crate) stream_type: StreamType,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub enum StreamType {
    Live,
    #[serde(rename = "On-Demand")]
    OnDemand,
}

// #[derive(Debug, Clone, PartialEq, Deserialize)]
// #[serde(rename_all = "snake_case")]
// pub enum Type {
//     Video,
// }

// #[derive(Debug, Clone, PartialEq, Deserialize)]
// pub enum ShowName {
//     #[serde(rename = "CBC Sports")]
//     CbcSports,
// }

// #[derive(Debug, Clone, PartialEq, Deserialize)]
// pub struct Extensions {
//     pub(crate) warnings: Vec<String>,
// }

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitialState {
    // #[serde(rename = "a11y")]
    // pub(crate) a11_y: A11Y,
    // pub(crate) app: App,
    // pub(crate) author: Author,
    // pub(crate) content: InitialStateContent,
    // pub(crate) cookie_jar: CookieJar,
    // pub(crate) detail: Detail,
    // pub(crate) featureflags: Featureflags,
    // pub(crate) feedback: Feedback,
    // pub(crate) fixed: Fixed,
    // pub(crate) flp: Flp,
    // pub(crate) gdpr: Gdpr,
    // pub(crate) live_radio: Option<serde_json::Value>,
    // pub(crate) loader: Loader,
    // pub(crate) navigation: Navigation,
    // pub(crate) newsletters: Newsletters,
    // pub(crate) page: Page,
    // pub(crate) persistent_player: PersistentPlayer,
    // pub(crate) personalization: Personalization,
    // pub(crate) plus: Plus,
    // pub(crate) preferences: Preferences,
    // pub(crate) regions: Regions,
    // pub(crate) right_rail: RightRail,
    // pub(crate) schedule: Schedule,
    // pub(crate) search: Schedule,
    // pub(crate) sectional_content: Schedule,
    // pub(crate) ssr: Ssr,
    // pub(crate) subject_content: SubjectContent,
    // pub(crate) tracking: InitialStateTracking,
    // pub(crate) trending: Trending,
    pub(crate) video: Video,
    // pub(crate) video_detail: VideoDetail,
    // pub(crate) weather: Weather,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Video {
    pub(crate) current_clip: CurrentClip,
    // pub(crate) recommendations: CuratedPlaylist,
    // pub(crate) trending_clips: CuratedPlaylist,
    // pub(crate) curated_playlist: CuratedPlaylist,
    // pub(crate) more_from_base_section: CuratedPlaylist,
}

impl Video {
    pub(crate) fn get_stream_urls(&self) -> StreamURLs {
        let mut urls = StreamURLs { dai: None, medianet: None };
        for surl in &self.current_clip.media.assets {
            if surl.asset_type == "platform-dai" {
                // TODO https://pubads.g.doubleclick.net
                //  it requires a bit more work but I don't know if medianet is always present
            } else if surl.asset_type == "medianet" {
                urls.medianet = Some(surl.key.to_string());
            }
        }
        urls
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StreamURLs {
    pub(crate) dai: Option<String>,
    pub(crate) medianet: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentClip {
    pub(crate) source_id: String,
    // pub(crate) media_id: Option<serde_json::Value>,
    pub(crate) source: String,
    pub(crate) title: String,
    // pub(crate) image: Image,
    pub(crate) published_at: String,
    // #[serde(rename = "type")]
    // pub(crate) current_clip_type: StreamType,
    // pub(crate) show_data: Option<serde_json::Value>,
    // pub(crate) show_name: Option<serde_json::Value>,
    // pub(crate) tags: Vec<Tag>,
    // pub(crate) concepts: Vec<Option<serde_json::Value>>,
    pub(crate) media: CurrentClipMedia,
    pub(crate) updated_at: String,
    pub(crate) description: String,
    // pub(crate) categories: Vec<Category2>,
    // pub(crate) section: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentClipMedia {
    pub(crate) id: i64,
    // pub(crate) call_sign: Option<serde_json::Value>,
    pub(crate) assets: Vec<Asset>,
    // pub(crate) ad_order: String,
    // pub(crate) ad_category_exclusion: Option<serde_json::Value>,
    // pub(crate) stream_type: StreamType,
    // pub(crate) content_area: String,
    // pub(crate) content_tier_id: i64,
    pub(crate) duration: i64,
    // pub(crate) genre: Option<serde_json::Value>,
    // pub(crate) clip_type: String,
    // pub(crate) branded_sponsor_name: String,
    // pub(crate) season: Option<serde_json::Value>,
    // pub(crate) episode: Option<serde_json::Value>,
    // pub(crate) region: String,
    // pub(crate) sports: Spo,
    // pub(crate) has_captions: bool,
    // pub(crate) aspect_ratio: String,
    // pub(crate) text_tracks: Vec<Option<serde_json::Value>>,
    // pub(crate) chapters: Option<serde_json::Value>,
    // pub(crate) exclude_from_recommendations: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Asset {
    pub(crate) key: String,
    #[serde(rename = "type")]
    pub(crate) asset_type: String,
    // pub(crate) options: Option<serde_json::Value>,
}

// #[derive(Debug, Clone, PartialEq, Deserialize)]
// pub struct Category2 {
//     pub(crate) name: String,
//     pub(crate) slug: String,
// }

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stream {
    pub(crate) url: String,
    // pub(crate) message: Option<serde_json::Value>,
    pub(crate) error_code: i64,
    pub(crate) params: Vec<Param>,
    // pub(crate) bitrates: Vec<Bitrate>,
}

// #[derive(Debug, Clone, PartialEq, Deserialize)]
// pub struct Bitrate {
//     pub(crate) bitrate: i64,
//     pub(crate) width: i64,
//     pub(crate) height: i64,
//     pub(crate) lines: String,
//     pub(crate) param: Option<serde_json::Value>,
//     pub(crate) max: i64,
// }

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Param {
    pub(crate) name: String,
    pub(crate) value: Value,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Integer(i64),
    String(String),
}
