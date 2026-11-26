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
| 5 | App: tray, owner window (web view), storage, start at login, single instance | done: installed and run on the owner's Mac; Windows VM (WebView2) |
| 6 | Real capture: macOS (owner's Mac), Windows (DXGI + Media Foundation, VM), Linux (X11 + OpenH264, VM) | done 2026-11-26 |
| 7 | Packaging and CI: signed macOS dmg, Windows exe, Linux x86_64 + ARM64; CI on all four | done 2026-11-26 |
| 8 | Releases HID-Fi's way (own release key, own macOS certificate, signed SHA256SUMS); in-app updates | done 2026-11-26 (D-09) |
| 9 | Trusted https; talk back | planned, needs the owner's DNS/Cloudflare accounts (docs/HTTPS.md) |

## Log

### 2026-11-26: phones, Windows, Linux, CI, the repo, the first release

Owner's live test on an iPhone: controls dead on its side, full screen did nothing,
the phone forgotten after scanning. Causes found: controls under the notch in landscape
(no safe-area insets), iPhone has no page full screen, and the camera opens an in-app
Safari view with its own storage. Fixed (insets everywhere, one-tap faded controls,
iPhone's own player with resume, a Home Screen guide) and tested in WebKit with an
iPhone's size, touch and a simulated notch. Pairing no longer needs BigInt (iOS < 14).
A legacy floor guard (Safari 12.1 / Chrome 70 / Firefox 68).

- Logo refined: continuous-curvature frame, the dot as its corner, one stroke family.
- Owner window: memory/CPU/uptime self-monitor; activity scrolls; only changed parts redraw.
- Picture choice per viewer: Auto, Sharp, Smooth, Saver.
- Endurance: a full log and device list; 40 viewers after warm-up (memory trend
  +0.25 MB/viewer, threads flat; a 100-viewer run swings 48-74 MB and falls back);
  a restart found again without pairing; a 6 s network stall recovers by itself.
- **Windows VM found two real bugs**: read deadlines set on a cloned socket handle did
  not reach the reading handle (a silent client held a thread forever), and an error
  reply was lost to a reset on close. Fixed; all browser tests pass there; the real app
  runs (WebView2, 51 MB); real DXGI capture + Media Foundation checked.
- **Linux VM (Docker, ARM64)**: build, 108 unit tests, clippy, the app, real X11 capture
  + Cisco OpenH264 paired and decoded by a Python WebRTC viewer (red left, blue right).
- **Latency probe corrected** (D-07, corrected): wall clock per frame, never before
  decode, every display phase. 1080p60 p50 19-20 ms, p99 27-28.5 ms.
- Repo public at github.com/san-gitlogin/Wi-Live; CI on macOS, Windows, Linux x86_64 + ARM64.
- Release keys of its own (~/.wilive-release), tools/make_release.sh, docs/RELEASING.md.

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

## Log, later on 2026-11-26

- Signing keys backed up to the Extreme SSD (AES-256 image, passphrase in the login
  keychain as "Release keys backup (SSD)"; README beside it).
- Updates (D-09), Linux tray + one-time owner link, Sharp at full size (D-08), iPhone
  picture in picture with a fallback and a reason, frame pickup on arrival, Windows icon,
  real-screen latency (D-10), probe made self-checking. Every tier passes; Linux VM passes.

## Next

1. Trusted https and talk back, once the owner gives the DNS/Cloudflare go-ahead
   (docs/HTTPS.md).
2. The Linux tray on a desktop with a tray host (KDE, or GNOME with its extension):
   checked in a container without one so far.
3. Windows self-update: proven in the VM (tests/windows/update_e2e.py); Linux self-update by the same swap, not yet run in the VM.
