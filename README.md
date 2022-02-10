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

[sl]: https://github.com/streamlink/streamlink

### Notes

* MPC-HC will occasionally fail to detect audio. Close and re-run to try again if this
  happens to you, whatever your player is.
* You can't seek, even in replays. Limitation of streamlink.
* It's 720p30 at a less-than-perfect bitrate.
