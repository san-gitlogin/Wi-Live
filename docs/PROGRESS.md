# Wi-Live: progress

Newest first. Read REQUIREMENTS.md, DESIGN.md and DECISIONS.md before continuing.

## Phases

| Phase | What | State |
|---|---|---|
| 0 | Requirements, design, decisions, brand (logo, colours, fonts) | done 2026-11-25 |
| 1 | Core: pairing crypto, requests, devices, access log (Rust + pure-JS page crypto, pinned vectors) | done 2026-11-25 |
| 2 | Server: one port, dashboard, pairing API, owner API, signaling, defences | done 2026-11-25 |
| 3 | Dashboard: viewer (pair, watch, displays, sound, fit, stats) and owner window | done 2026-11-25 |
| 4 | Tests: static, unit (+ planted bugs), e2e in real Chrome, security, tampering, latency gate | done 2026-11-25 |
| 5 | App: tray, owner window (web view), storage, start at login, single instance | built + unit-tested; GUI not yet run on screen |
| 6 | Real capture check on this Mac (needs Screen Recording for a signed app) | todo |
| 7 | Packaging: signed macOS app, Windows exe, Linux; CI on all platforms | todo |
| 8 | Signed updates and releases, HID-Fi's way | todo |
| 9 | Trusted https (per-install name + real certificate); talk back (R-14) | todo |

## Log

### 2026-11-25: from nothing to a measured, tested first build

- Owner's brief and answers: name "Wi-Live", PolyForm Noncommercial like HID-Fi,
  local git until they say push.
- Forked `hidfi-stream` as `wilive-stream`, removing the board coupling. Ticket admission
  became Wi-Live's own: numeric-comparison pairing approved on the computer, a
  challenge-response every session, and a fingerprint proof keyed to the device (D-04).
- **Latency, measured** (tests/latency, real Chrome, loopback). A time strip in the
  test picture is read back from every presented frame. The first measurement split
  out where the time went, and decode-to-display was 28 ms of 42. Sending playout
  delay 0/0 (D-07) brought 1080p60 to p50 19-26 / p99 27-28 ms, with all 60 fps shown.
- **Bugs found by the tests and fixed before anything shipped:**
  - Strikes blocked a whole address, including loopback, so the owner's own window
    was locked out. A removed phone retrying would have locked out a whole household.
    Now loopback is never blocked, a blocked address still gets its page and a reason,
    and an unknown (removed) device is not counted as an attack.
  - A viewer the owner ended reconnected by itself, so "End" now waits for "Watch again".
  - A viewer leaving was logged as a capture failure, from a race in the forwarder.
  - A two-tap "Remove?" could wait behind a poll, so it now redraws at once.
- **Screenshots reviewed** in light and dark at desktop and phone sizes. Red was used
  for toggles and cards, which is now neutral. Red marks live and the one primary action.
- Results: static 12, unit 9 (Rust 49 tests and 31 JS crypto checks, 7 planted crypto
  bugs all caught), e2e 34 (pairing, ending, removing, pause, two viewers, displays and
  unplug, owner window, 15 server defences, 3 tampering cases), latency 2/2.

## Next

1. Run the tray app on screen with the owner (it opens its window on first run).
2. A signed macOS bundle, so Screen Recording can be granted once and kept; then the
   real-capture latency check (a clock page on screen, read back by the probe) and a
   clarity pass at native resolution (MAX_EDGE is 1920 today, inherited from HID-Fi).
3. A QR code in the owner window (phone opens the address by camera).
4. CI on Windows and Linux; installers; the release flow with signed updates.
5. Linux tray (the server runs headless there today).
