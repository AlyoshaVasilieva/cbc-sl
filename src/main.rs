use std::path::PathBuf;
use std::process::Command;

use anyhow::{anyhow, ensure, Context, Result};
use clap::Parser;
use extend::ext;
use hls_m3u8::{tags::VariantStream, MasterPlaylist};
use lazy_regex::{lazy_regex, regex};
use once_cell::sync::Lazy;
use owo_colors::{OwoColorize, Stream::Stdout};
use regex::Regex;
use serde_json::json;
use ureq::{Agent, AgentBuilder, Proxy};
use url::Url;

use crate::api::{InitialState, Stream};

mod api;
#[cfg(windows)]
mod wincolors;

// pretend to be a real browser
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
(KHTML, like Gecko) Chrome/127.0.0.0 Safari/537.36";

static ID_REGEX: Lazy<Regex> =
    lazy_regex!(r#"(?:https://www\.cbc\.ca/player/play/video/)?([[:digit:]]+\.[[:digit:]]+)"#);

#[derive(Debug, Parser)]
#[clap(version)]
#[clap(about)]
struct Args {
    /// Proxy to use (if you aren't in Canada). If no scheme is set, defaults to socks5
    #[clap(short = 'p', long = "proxy")]
    proxy: Option<String>,
    /// Don't run streamlink, just print the stream URL. Note that CBC.ca requires a matching
    /// User-Agent or it will reject your request
    #[clap(short = 'n', long = "no-run", conflicts_with_all(&["list", "replays"]))]
    no_run: bool,
    /// List available Olympics streams
    #[clap(short = 'l', long = "list", conflicts_with_all(&["url", "replays"]))]
    list: bool,
    /// List available Olympics replays (at most 24 are shown)
    #[clap(short = 'a', long = "replays", conflicts_with_all(&["url", "list"]))]
    replays: bool,
    /// Streamlink log level
    #[clap(long = "loglevel", value_parser(["none", "error", "warning", "info", "debug", "trace"]), default_value = "info")]
    loglevel: String,
    /// Don't trust streamlink to handle the master playlist. Works around a bug in certain old
    /// versions of streamlink. This shouldn't do anything on versions >3.1.1.
    #[clap(short = 'T', long = "distrust-streamlink")]
    distrust: bool,
    /// Stream quality to request. Won't work if you're using --distrust-streamlink
    #[clap(short = 'q', long = "quality", default_value = "best")]
    quality: String,
    /// Streamlink bin name or path
    #[clap(short = 'S', long = "streamlink", default_value = "streamlink")]
    streamlink: PathBuf,
    /// Show full URLs when listing events
    #[clap(short = 'f', long = "full-urls")]
    full_urls: bool,
    /// CBC.ca URL or ID
    #[clap(value_parser(probably_cbc), required_unless_present_any(["list", "replays"]))]
    url: Option<String>,
}

fn get_live_and_upcoming(agent: &Agent) -> Result<api::GqlResponse> {
    const LIVE_QUERY: &str =
        "query contentItemsByItemsQueryFilters($itemsQueryFilters:ItemsQueryFilters\
    ,$page:Int,$pageSize:Int,$minPubDate:String,$maxPubDate:String,$lineupOnly:Boolean,$offset:Int)\
    {allContentItems(itemsQueryFilters:$itemsQueryFilters,page:$page,pageSize:$pageSize,offset:\
    $offset,minPubDate:$minPubDate,maxPubDate:$maxPubDate,lineupOnly:$lineupOnly,targets:[WEB,ALL])\
    {nodes{...cardNode}}}fragment cardNode on ContentItem{id url title sectionList sectionLabels \
    relatedLinks{url title sourceId}deck description flag imageLarge image{_16x9_460:derivative\
    (preferredWidth:460,aspectRatio:\"16x9\"){w fileurl}_16x9_620:derivative(preferredWidth:620,\
    aspectRatio:\"16x9\"){w fileurl}_16x9_940:derivative(preferredWidth:940,aspectRatio:\"16x9\")\
    {w fileurl}square_220:derivative(preferredWidth:220,aspectRatio:\"square\"){w fileurl}}source \
    sourceId publishedAt updatedAt sponsor{name logo url external label}type showName authors{name \
    smallImageUrl}commentsEnabled contextualHeadlines{headline contextualLineupSlug}mediaId media\
    {duration hasCaptions streamType}headlineData{type title mediaId sourceId mediaDuration \
    publishedAt image}components{mainContent{url sectionList flag sourceId type}mainVisual{...on \
    ContentItem{publishedAt mediaId sourceId media{duration hasCaptions streamType}title \
    imageLarge}}primary secondary tertiary}categories{name slug path}}";

    let query = json!({
        "query": LIVE_QUERY,
        "variables": {
            "lineupOnly": false,
            "page": 1,
            "pageSize": 15,
            "maxPubDate": "now+35d",
            "minPubDate": "now-14h",
            "itemsQueryFilters": {
                "types": [
                    "video"
                ],
                "categorySlugs": [
                    "summer-olympics-live"
                ],
                "sort": "+publishedAt",
                "mediaStreamType": "Live"
            }
        }
    });

    Ok(agent.post("https://www.cbc.ca/graphql").send_json(query)?.into_json()?)
}

fn get_replays(agent: &Agent) -> Result<api::GqlResponse> {
    const VOD_QUERY: &str = "query contentItemsByItemsQueryFilters($itemsQueryFilters:\
    ItemsQueryFilters,$page:Int,$pageSize:Int,$minPubDate:String,$maxPubDate:String,\
    $lineupOnly:Boolean,$offset:Int){allContentItems(itemsQueryFilters:$itemsQueryFilters,\
    page:$page,pageSize:$pageSize,offset:$offset,minPubDate:$minPubDate,maxPubDate:$maxPubDate,\
    lineupOnly:$lineupOnly,targets:[WEB,ALL]){nodes{...cardNode}}}fragment cardNode on \
    ContentItem{id url title sectionList sectionLabels relatedLinks{url title sourceId}deck \
    description flag imageLarge image{_16x9_460:derivative(preferredWidth:460,aspectRatio:\"16x9\")\
    {w fileurl}_16x9_620:derivative(preferredWidth:620,aspectRatio:\"16x9\"){w fileurl}_16x9_940:\
    derivative(preferredWidth:940,aspectRatio:\"16x9\"){w fileurl}square_220:derivative\
    (preferredWidth:220,aspectRatio:\"square\"){w fileurl}}source sourceId publishedAt updatedAt \
    sponsor{name logo url external label}type showName authors{name smallImageUrl}commentsEnabled \
    contextualHeadlines{headline contextualLineupSlug}mediaId media{duration hasCaptions \
    streamType}headlineData{type title mediaId sourceId mediaDuration publishedAt image}components\
    {mainContent{url sectionList flag sourceId type}mainVisual{...on ContentItem{publishedAt \
    mediaId sourceId media{duration hasCaptions streamType}title imageLarge}}primary secondary \
    tertiary}categories{name slug path}}";

    let query = json!({
        "query": VOD_QUERY,
        "variables": {
            "lineupOnly": false,
            "page": 1,
            "pageSize": 16,
            "itemsQueryFilters": {
                "types": [
                    "video"
                ],
                "sort": "-publishedAt",
                "categorySlugs": [
                    "summer-olympics-replays"
                ]
            }
        }
    });
    Ok(agent.post("https://www.cbc.ca/graphql").send_json(query)?.into_json()?)
}

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
        for item in get_live_and_upcoming(&agent)?.data.all_content_items.nodes {
            println!("{}", item.to_human(args.full_urls)?);
        }
        return Ok(());
    }
    if args.replays {
        for item in get_replays(&agent)?.data.all_content_items.nodes {
            println!("{}", item.to_human(args.full_urls)?);
        }
        return Ok(());
    }

    let id = parse_cbc_id(&args.url.unwrap())?;

    let target = format!("https://www.cbc.ca/player/play/video/{id}");
    let page = agent.get(&target).call()?.into_string()?;
    let preload_json_regex = regex!(r#"window\.__INITIAL_STATE__ = (.*);</script>"#);
    let preload_json = preload_json_regex
        .captures(&page)
        .ok_or_else(|| anyhow!("couldn't find initial state!"))?
        .get(1)
        .unwrap()
        .as_str();
    let initial_state: InitialState = serde_json::from_str(preload_json)?;
    let surls = initial_state.video.get_stream_urls();
    let json_url = surls.medianet.ok_or_else(|| anyhow!("no medianet URL found"))?;

    let blocked = format!(
        "grabbing stream data; an error here probably means {}",
        "your IP is geo-blocked".if_supports_color(Stdout, |text| text.bright_red().on_black()),
    );

    let stream_json: Stream = agent.get(&json_url).call()?.into_json().context(blocked)?;
    let master_url = stream_json.url.as_str();

    let stream = if args.distrust {
        let playlist = agent.get(master_url).call()?.into_string()?;
        get_best_stream(master_url, &playlist)?
    } else {
        master_url.to_owned()
    };
    if args.no_run {
        println!("User-Agent: {}", USER_AGENT);
        println!("URL: {}", stream);
    } else {
        let sl = args.streamlink;
        let mut cmd = Command::new(sl);
        cmd.arg("--loglevel")
            .arg(&args.loglevel)
            .arg("--http-header")
            .arg(format!("User-Agent={USER_AGENT}"))
            .arg("--http-header")
            .arg(format!("Referer={target}"));
        let stat = if let Some(proxy) = args.proxy.map(|p| proxy_url_streamlink(&p)) {
            cmd.arg("--http-proxy").arg(&proxy).arg(stream).arg(args.quality).status()?
        } else {
            cmd.arg(stream).arg(args.quality).status()?
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
fn probably_cbc(input: &str) -> std::result::Result<String, String> {
    if let Some(cap) = ID_REGEX.captures(input) {
        Ok(cap.get(1).unwrap().as_str().to_string())
    } else {
        Err("invalid url".into())
    }
}

fn parse_cbc_id(input: &str) -> Result<String> {
    Ok(ID_REGEX.captures(input).unwrap().get(1).unwrap().as_str().to_string())
}
