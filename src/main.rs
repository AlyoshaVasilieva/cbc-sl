use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use owo_colors::OwoColorize;
use owo_colors::Stream::Stdout;
use ureq::{Agent, Proxy};
use url::Url;

use crate::api::{Item, ItemType, LineupCategory, Olympics, Stream};

mod api;
#[cfg(windows)]
mod wincolors;

// pretend to be a real browser
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:140.0) \
Gecko/20100101 Firefox/140.0";

// https://gem.cbc.ca/section/olympics

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
    /// List available Olympics replays
    #[clap(short = 'r', long = "replays", conflicts_with_all(&["url", "list"]))]
    replays: bool,
    /// Streamlink log level
    #[clap(long = "loglevel", value_parser(["none", "error", "warning", "info", "debug", "trace"]), default_value = "info")]
    loglevel: String,
    /// Stream quality to request
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

fn get_live_and_upcoming(agent: &Agent) -> Result<Vec<Item>> {
    get_items(agent, LineupCategory::LiveUpcoming, "Live & Upcoming")
}

fn get_replays(agent: &Agent) -> Result<Vec<Item>> {
    get_items(agent, LineupCategory::Replays, "Replays")
}

fn get_items(agent: &Agent, category: LineupCategory, cat_name: &str) -> Result<Vec<Item>> {
    const URL: &str = "https://services.radio-canada.ca/ott/catalog/v2/gem/section/olympics";

    let api: Olympics = agent
        .get(URL)
        .query("device", "web")
        .query("pageSize", "6") // appears to affect what I'm calling "categories", not item lists
        .query("pageNumber", "1")
        .call()?
        .body_mut()
        .read_json()?;

    // TODO: As the Olympics progresses this might need pagination

    let mut streams = api
        .lineups
        .results
        .into_iter()
        .find(|lineup| lineup.category == category)
        .ok_or_else(|| anyhow!("failed to find {cat_name}"))?
        .items
        .ok_or_else(|| anyhow!("{cat_name} is missing items"))?;

    // keep only streams; note that Live just means "this aired/is airing/will air"
    streams.retain(|lu| lu.item_type == ItemType::Live);

    Ok(streams)
}

fn main() -> Result<()> {
    let args = Args::parse();
    #[cfg(windows)]
    let _ = wincolors::enable_colors();
    let mut ab = Agent::config_builder().user_agent(USER_AGENT);
    if let Some(proxy) = args.proxy.as_deref() {
        ab = ab.proxy(Some(Proxy::new(&proxy_url_ureq(proxy))?));
    }
    let agent = Agent::from(ab.build());
    if args.list {
        for item in get_live_and_upcoming(&agent)? {
            println!("{}", item.to_human(true, args.full_urls)?);
        }
        return Ok(());
    }
    if args.replays {
        for item in get_replays(&agent)? {
            println!("{}", item.to_human(false, args.full_urls)?);
        }
        return Ok(());
    }

    let id = &args.url.unwrap();

    let blocked = format!(
        "grabbing stream info; an error here probably means {}",
        "your IP is geo-blocked".if_supports_color(Stdout, |text| text.bright_red().on_black()),
    );

    let stream_info: Stream = agent
        .get("https://services.radio-canada.ca/media/validation/v2/")
        .query("appCode", "medianetlive")
        .query("connectionType", "hd")
        .query("deviceType", "ipad")
        .query("idMedia", id)
        .query("multibitrate", "true")
        .query("output", "json")
        .query("tech", "hls")
        .query("manifestVersion", "2")
        .query("manifestType", "desktop")
        .call()?
        .body_mut()
        .read_json()
        .context(blocked)?;
    let stream = stream_info.url.as_str();

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
            .arg("Referer=https://gem.cbc.ca/");
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
    let numeric = !input.is_empty() && input.chars().all(|c| c.is_ascii_digit());

    if numeric {
        Ok(input.to_string())
    } else {
        match parse_cbc_url_to_id(input) {
            Ok(id) => Ok(id),
            _ => Err("invalid url".into()),
        }
    }
}

fn parse_cbc_url_to_id(input: &str) -> Result<String> {
    let url = Url::parse(input)?;
    let last = url
        .path_segments()
        .ok_or_else(|| anyhow!("missing path"))?
        .next_back()
        .ok_or_else(|| anyhow!("URL missing path"))?;
    let ridx = last.rfind(['-', '/']).ok_or_else(|| anyhow!("URL segment doesn't have ID"))?;
    Ok(last[ridx + 1..].to_string())
}

#[cfg(test)]
mod tests {
    use crate::{parse_cbc_url_to_id, probably_cbc};

    #[test]
    fn test_parse_cbc_url() {
        let input = "https://gem.cbc.ca/curling-norway-vs-canada-mixed-doubles-round-robin-30045";
        assert_eq!(parse_cbc_url_to_id(input).unwrap(), "30045");
    }

    #[test]
    fn test_probably_cbc() {
        let input = "https://gem.cbc.ca/curling-norway-vs-canada-mixed-doubles-round-robin-30045";
        assert_eq!(probably_cbc(input).unwrap(), "30045");
        assert_eq!(probably_cbc("30045").unwrap(), "30045");
    }
}
