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

### D-07, corrected (2026-11-26): the probe's clock, and every display phase

Two measurement faults, found because a run reported frames shown before they
arrived. (1) `performance.timeOrigin` drifts from the wall clock while a page stays
open; the probe now reads the wall clock at each frame. (2) Headless Chrome sometimes
names an earlier refresh as a frame's display time; a frame now counts as shown no
earlier than its decode finished. And at 60 fps on a 60 Hz display the frame-to-refresh
alignment is fixed for a whole run, so each run measured one random phase (1080p p50
anywhere from 22 to 39 ms). The gate now runs at 59 fps, sweeping every phase each second.

Honest figures (loopback, 4 runs): 1080p p50 19-20 ms, p99 27-28.5 ms. 720p: either
p50 11-12 / p99 20 or p50 31-32 / p99 40-41 per run: some runs pay one extra compositor
frame in headless Chrome. Open: find why (720p only, never 1080p).

## D-08 Fast by default, full size on request (2026-11-26)

Owner: "more granular control over what we are viewing", and lag first. Measured with
tests/latency: 1080p p50 19-21 ms, p99 27-30 ms; a 3024x1964 screen sent whole p50
35-58 ms at about 26 fps. So the capture is capped at 1920 on the long edge (Auto, the
default), and a viewer who chooses **Sharp** gets the screen at its own size, up to 4K:
that display's capture reopens at that size while anyone watching it wants Sharp. Smooth
(at most half) and Saver (a quarter) cap the size for that viewer only.

## D-09 Updates: signed, checked, installed only when the owner asks (2026-11-26)

The app reads the latest release's SHA256SUMS and signature once a day; the signature is
checked against the release key built into the app before anything is read; the file for
this computer must match the signed list; on macOS the new app must also carry Wi-Live's
own certificate. Only the owner's "Install and restart" installs (atomic swap, the old copy
kept beside it until the next start). Releases are published as full releases, because
GitHub's `releases/latest` skips pre-releases. Proven end to end on this Mac with the
real signed v0.1.0 (tests/e2e/test_update_mac.py), including a tampered list and a
corrupted download.

## D-10 Real-screen latency is measured too (2026-11-26)

tests/real_latency.py: a visible clock window, captured by the installed app, read
back by a paired viewer. 1920x1246 at 56 fps: p50 52-72 ms, p99 69-82 ms across five
runs. This includes the clock window's own paint and macOS's capture (about 30 ms more
than the synthetic picture), which Wi-Live cannot remove; picking frames up the moment
they arrive raised the frames delivered from 52 to 56 fps, with no change in median
latency beyond noise. Not gated (it needs a screen, a window on top and an allowed
viewer); run by hand before a release.
