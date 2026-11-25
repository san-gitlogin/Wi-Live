# AI agent instructions: Wi-Live

Read this before changing anything. Wi-Live shares a computer's screen, sound and
microphone with devices in the same home network (docs/DESIGN.md). It is built from
HID-Fi Companion's streaming code. Most of HID-Fi's rules apply here because they came
from things that went wrong.

## Layout

```
crates/wilive-core     pairing crypto, requests, devices, access log (pure, no OS)
crates/wilive-stream   capture, encode, sound, WebRTC (fork of hidfi-stream)
crates/wilive-server   one port: dashboard files, pairing + owner API, signaling
crates/wilive          the app: tray, owner window, storage, start at login
web/                   the dashboard (viewer + owner), built into the app
vectors/               pinned pairing bytes: Rust and JS must match them
vendor/str0m           str0m with HID-Fi's bandwidth fixes (PATCH.md)
tests/                 run.py is the one command; tiers in pytest.ini
docs/                  DESIGN, DECISIONS, REQUIREMENTS, PROGRESS, BRAND
brand/                 logo, tray icon, make-icons.sh
```

## Commands

```
python3 tests/run.py           static + unit (fmt, clippy -D warnings, cargo test, JS crypto)
python3 tests/run.py --all     + e2e, security, tampering and latency in headless Chrome
cargo run -p wilive-server --example serve   the server with a test picture (prints port + owner token)
```

## Non-negotiables

1. **Latency is measured, never claimed** (D-03). A change on the media path runs
   `tests/run.py --tier latency`. A budget is raised only with the owner.
2. **Nothing queues.** Frames are dropped, never buffered. Queued video is lag that
   never recovers.
3. **Paired devices only.** Being on the network is never enough. The owner sees every
   viewer and can end it, and the computer shows a live mark while anyone watches.
4. **Secrets never leave.** Device keys stay in `devices.json` (owner-only) and in the
   device's own storage. The owner token is in memory and given only to the window.
   Nothing logs a key, proof, code or token (static guard).
5. **The page loads nothing from the internet** and passes its own CSP: no inline
   style or script, no eval, text only through textContent.
6. **Both themes meet WCAG AA.** Red means live and the one primary action (docs/BRAND.md).
7. **New behaviour ships with its test**, and a security claim ships with a test that
   tries to break it. A planted bug must be caught.
8. **Look at the picture.** Green tests can pass on the wrong picture. Take screenshots
   in light and dark, at phone and desktop sizes, and look at them.
9. **Outside facts are checked at the source**, on the day (versions, browser behaviour).
10. **Never push, tag or publish without the owner's OK.** Commit locally freely.
