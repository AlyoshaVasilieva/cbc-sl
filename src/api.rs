use anyhow::{Result, anyhow};
use jiff::{Timestamp, Zoned, tz::TimeZone};
use owo_colors::{OwoColorize, Stream::Stdout};
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Olympics {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) lineups: Lineups,
}

// #[derive(Debug, Clone, PartialEq, Deserialize)]
// pub struct AppleMediaServiceSubscriptionV2 {
//     pub(crate) expires: i64,
//     #[serde(rename = "type")]
//     pub(crate) apple_media_service_subscription_v2_type: TypeClass,
// }

// #[derive(Debug, Clone, PartialEq, Deserialize)]
// #[serde(rename_all = "camelCase")]
// pub struct TypeClass {
//     pub(crate) availability_type: String,
//     pub(crate) tiers: String,
// }

// #[derive(Debug, Clone, PartialEq, Deserialize)]
// pub struct Background {
//     pub(crate) url: String,
//     pub(crate) size: Size,
// }

// #[derive(Debug, Clone, PartialEq, Deserialize)]
// pub enum Size {
//     Bigger,
//     Normal,
// }

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lineups {
    pub(crate) total_pages: i64,
    pub(crate) total_records: i64,
    pub(crate) page_number: i64,
    pub(crate) page_size: i64,
    pub(crate) results: Vec<ResultA>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultA {
    #[serde(rename = "title")]
    pub(crate) category: LineupCategory,
    pub(crate) key: String,
    pub(crate) items: Option<Vec<Item>>,
    // pub(crate) card_image_type: String,
    // pub(crate) layout_type: String,
    // pub(crate) lineup_type: String,
    // pub(crate) not_signed_in_message: Option<String>,
    // pub(crate) images: Option<ResultImages>,
}

// TODO: See if there's a better API that's less bad

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) enum LineupCategory {
    #[serde(rename = "Featured Olympic Content Milan")]
    Featured,
    #[serde(rename = "Live & Upcoming")]
    LiveUpcoming,
    #[serde(rename = "My Olympics")]
    MyOlympics,
    #[serde(rename = "Browse by Sport")]
    BySport,
    #[serde(rename = "Highlights")]
    Highlights,
    #[serde(rename = "Replays")]
    Replays,
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub(crate) title: String,
    pub(crate) key: String,
    pub(crate) description: Option<String>,
    pub(crate) tier: Option<String>,
    pub(crate) url: String,
    #[serde(rename = "type")]
    pub(crate) item_type: ItemType,
    // pub(crate) feed_type: Option<FeedType>,
    // pub(crate) granted_right: GrantedRight,
    // pub(crate) closed_caption_available: bool,
    // pub(crate) video_description_available: bool,
    // pub(crate) is_playback_status_supported: Option<bool>,
    // pub(crate) is_vod_enabled: Option<bool>,
    pub(crate) air_date: Option<Timestamp>,
    // pub(crate) is_legacy_live_event: Option<bool>,
    // pub(crate) info_title: Option<String>,
    // pub(crate) badge: Option<Badge>,
    // pub(crate) id_media: Option<i64>,
    pub(crate) formatted_id_media: Option<String>,
}

impl Item {
    pub(crate) fn get_id(&self) -> String {
        self.formatted_id_media.clone().unwrap_or_else(|| {
            let ridx = self.url.rfind(['-', '/']).expect("doubly missing ID");
            self.url[ridx + 1..].to_string()
        })
    }

    /// `lu` = "is this item live/upcoming"
    /// can't autodetect if it's live or a replay here
    pub(crate) fn to_human(&self, lu: bool, full_urls: bool) -> Result<String> {
        let now = Zoned::now();
        let air_date = self.zoned(TimeZone::system()).ok_or_else(|| anyhow!("missing air date"))?;
        // TODO: Show ??:?? or something instead
        let same_day = now.date() == air_date.date();
        let fmt = if same_day { "%H:%M" } else { "%b %d %H:%M" };
        let is_live = lu && air_date < now;
        // best guess, it's no longer in the API AFAICT

        let date_time = air_date.strftime(fmt);

        let note = if lu {
            match is_live {
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
        let prefix = if full_urls { "https://gem.cbc.ca/" } else { "" };
        Ok(format!("{prefix}{} - {note}{}", self.get_id(), self.title))
    }

    pub(crate) fn zoned(&self, tz: TimeZone) -> Option<Zoned> {
        self.air_date.map(|ts| ts.to_zoned(tz))
    }
}

// #[derive(Debug, Clone, PartialEq, Deserialize)]
// pub struct Badge {
//     pub(crate) message: String,
//     #[serde(rename = "type")]
//     pub(crate) badge_type: String,
// }

// #[derive(Debug, Clone, PartialEq, Deserialize)]
// pub enum FeedType {
//     #[serde(rename = "LiveEvent")]
//     LiveEvent,
// }

// #[derive(Debug, Clone, PartialEq, Deserialize)]
// pub enum GrantedRight {
//     None,
// }

#[derive(Copy, Debug, Clone, PartialEq, Deserialize)]
pub enum ItemType {
    Collection,
    Live,
    Media,
    Section,
    Show,
}

// #[derive(Debug, Clone, PartialEq, Deserialize)]
// pub enum Tier {
//     Member,
//     Standard,
//     Premium,
// }

// pub(crate) enum LineupType {
//     Featured,
// }

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stream {
    pub(crate) url: String,
    pub(crate) message: Option<serde_json::Value>,
    pub(crate) error_code: i64,
    // pub(crate) params: Vec<StreamParam>,
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

// #[derive(Debug, Clone, PartialEq, Deserialize)]
// pub struct StreamParam {
//     pub(crate) name: String,
//     pub(crate) value: ParamValue,
// }
//
// #[derive(Debug, Clone, PartialEq, Deserialize)]
// #[serde(untagged)]
// pub enum ParamValue {
//     Integer(i64),
//     String(String),
// }
