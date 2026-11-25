# Decisions

Why Wi-Live is the way it is, in the owner's words where they gave them. A change
that goes against an entry is a new entry that revises it, never a quietly edited
test.

## D-01 No board, no cloud (2026-11-25)

Owner: "the only purpose of screen sharing via a home network ... no involvement of
any ESP, just does things over wifi, and we have a local URL to visit and see the
screen whenever and wherever within the home network."

The computer serves the dashboard, the API and the media. Only private-network
peers are answered (RFC 1918, link-local, ULA, loopback). Nothing is sent to the
internet.

## D-02 WebRTC, hardware H.264, no queue anywhere (2026-11-25)

Owner: "I can't tolerate any sort of lags - I need absolutely 0 latency and p99."

WebRTC brings encryption, congestion control and hardware decoding in every
browser. The fork of HID-Fi's `hidfi-stream` brings the rate control proven on
real WiFi. Frames are dropped, never queued. The browser's jitter buffer target is 0.

## D-03 Latency is measured, never claimed (2026-11-25)

Zero is not physically reachable, so it is turned into a number that a test can
fail: capture-to-present p50/p95/p99 from a time strip drawn into the test picture.
The budgets are in DESIGN "Latency". Lowering a budget is an entry here, and raising
one needs the owner.

## D-04 Paired devices only; the owner always sees and can end (2026-11-25)

Owner: "solid information displayed on who's accessing, security aspects."

Pairing by numeric comparison with commitment, approved on the computer.
Challenge-response on every session. A Live mark on the computer while anyone
watches. One-tap end, forget and pause.

## D-05 Plain http first; trusted https next (2026-11-25)

A self-signed certificate shows every viewer a warning page, which would be putting
the problem on the user (owner, 2026-11-25: "Nothing must be put on the user"). So the page
is served over http, with pure-JS crypto checked against RFC vectors and the media
still encrypted. Trusted https (a per-install name with a real certificate) is the
next step. It also enables talk back (R-14).

## D-06 One look, its own: red and warm whites (2026-11-25)

Owner: "different coloured theme than HID-Fi, maybe with red and shades of white
... appropriate fonts and new logo that is not childish, but professional."
docs/BRAND.md.

## D-07 Playout delay 0/0: show each frame the moment it is decoded (2026-11-25)

Measured in Chrome with tests/latency, splitting each frame's time with
`requestVideoFrameCallback` (`receiveTime`, `processingDuration`): at 1080p60, encode
and network took 13 ms, decode 1 ms, and **decode to display 28 ms**. That last part
was the browser smoothing frame timing, and only 44 of 60 frames were presented. Every
video frame now carries the RTP playout-delay extension with min = max = 0, the setting
cloud gaming uses. Chrome then renders frames as they decode.

| loopback, 3 runs | before p50 / p99 | after p50 / p99 |
|---|---|---|
| 1920x1080@60 | 42.0 / 43.4 ms, 44 fps shown | 19-26 / 27.1-28.5 ms, 60 fps shown |
| 1280x720@60 | 39.4 / 41.0 ms | 9-32 / 10.6-33.1 ms |

What remains is where a frame lands against the display's refresh (0 to 1 frame),
which no sender can remove. The cost is that on a jittery link, frames are shown
unevenly instead of late. If a viewer ever wants "smooth" over "now", it would
become a per-viewer choice, never the default.
