## cbc-sl, a streamlink helper for watching the Olympics on CBC.ca

I wanted to watch the Olympics on a non-NBC channel. CBC was easy to get working
in a browser with a Canadian proxy, but I wanted to use a proper media player,
so I wrote this.

This is quick and very dirty. I don't intend to expand it beyond watching the Olympics
or fix it if it breaks once I'm done watching them. (Update: Fixed for Beijing 2022.)

### Guide

1. Install [streamlink][sl] and configure it
2. Build from source (`cargo install --git https://github.com/AlyoshaVasilieva/cbc-sl`)
   or [download a pre-built binary](https://github.com/AlyoshaVasilieva/cbc-sl/releases)
3. Run `cbc-sl --list` or `cbc-sl --replays` to see what you can watch
4. Run `cbc-sl ID` to call streamlink

You can also use URLs, such as `cbc-sl https://www.cbc.ca/player/play/1920742467707`

Use `-p IP:port` option to specify a proxy. Supports HTTP, SOCKS4A, SOCKS5H proxies.

Additionally, with something like [yt-dlp] you can use this to download replays.

1. Use `cbc-sl --no-run ID` to show the URL and a permitted User-Agent
2. Download with `yt-dlp --user-agent AGENT URL`

[sl]: https://github.com/streamlink/streamlink
[yt-dlp]: https://github.com/yt-dlp/yt-dlp

### Notes

* If your player fails to detect audio, look for a configuration option like
  "Stream Analysis Duration" and increase it.
* You can't seek, even in replays. Limitation of streamlink.
* It's 720p30 at a less-than-perfect bitrate.
