# Wi-Live: design

The decisions and why they were made. When a decision changes, add an entry to
`DECISIONS.md` that revises it, and update this file. Requirement IDs (R-xx) refer to
`REQUIREMENTS.md`.

## What it is

Wi-Live is an app on a computer that shows that computer's screen, sound and
microphone live on any device in the same home network. The device only needs a
browser: you open the address the app shows (or scan its QR code) and watch.

There is no board, no cloud and no account. Everything stays inside the home
network, and the computer is the only server.

```
 phone / tablet / TV / another computer (any browser)
   │  1. http://<computer>:47900/          the dashboard, served by the app
   │  2. pair once: numeric comparison, the owner taps Allow on the computer
   │  3. ws /v1/signal: challenge + HMAC proof, then the SDP offer and answer
   ▼
 computer: wilive (Rust, tray + window)
   ──▶ 4. WebRTC media, DTLS-SRTP encrypted: H.264 video, Opus sound and microphone
```

It is built from what HID-Fi Companion learned in v4.0 to v4.5. Its capture, encode,
rate control and WebRTC code is a fork of `hidfi-stream` (`crates/wilive-stream`).
HID-Fi's board was the directory and the trust anchor. Here the computer is both, so
pairing and admission are new (see "Trust" below).

## Latency is the product (R-01..R-05)

The aim is that a viewer cannot tell the picture is late. Zero latency cannot be
done: light, the encoder and the display refresh each take time. So the target is a
number we measure, never one we claim:

| path | p50 | p99 |
|---|---|---|
| this computer, loopback (the e2e gate) | <= 35 ms | <= 60 ms |
| 5 GHz home WiFi, 1080p60 (field check) | <= 50 ms | <= 90 ms |

**What is measured.** The time from capture to the frame being presented in the
viewer. The test source draws its capture time into the picture as a strip of
blocks. The viewer reads that strip back from each presented frame, using
`requestVideoFrameCallback` and a canvas, and compares it with the same clock. This
is `tests/latency`. It reports p50, p95 and p99 and fails above the budget. A change
that makes those numbers worse fails the run, just as a broken feature would.

**Where the milliseconds go, and what each step does about it:**

- **Capture.** ScreenCaptureKit, DXGI or X11 Damage. Only the newest frame is kept,
  and nothing is queued (HID-Fi: "drops instead of queueing").
- **Encode.** The OS's hardware H.264 encoder. It uses real-time mode, no B-frames
  or reordering, constrained baseline, and a one-frame VBV, so a frame is sent the
  moment it is done.
- **Frame rate.** 60 fps by default where the encoder keeps up. A 60 Hz frame is
  16.7 ms, and at 30 fps half a frame alone is 17 ms of waiting.
- **Network.** UDP via str0m (ICE-lite, with no TURN on a LAN). The pacer never
  holds a keyframe behind a low estimate (`vendor/str0m/PATCH.md`).
- **Viewer.** Every video frame carries playout delay 0/0, so Chrome renders it the
  moment it decodes (D-07: decode-to-display went from 28 ms to about 10). Also
  `jitterBufferTarget = 0`. Nothing waits on audio sync for the picture.
- **Slow links.** The stream drops frames and lowers the rate. It never builds a
  queue: queued video is lag that never recovers.

## Trust (R-20..R-29)

**Threat model.** The home network is shared: a guest phone, a smart TV, a
roommate's laptop. Being on the network must never be enough to watch. Anyone who
watches is a device the owner allowed. The owner can always see who is watching,
and can end it.

**Pairing (once per device):** numeric comparison with a commitment, the Bluetooth LE
Secure Connections pattern HID-Fi uses for its Companion.

1. The viewer makes an X25519 key and a 16-byte nonce `nV`. It sends `POST
   /api/pair/start {name, v_pub, commit}`, where `commit = H("wl-commit" || v_pub || nV)`.
2. The app adds a pending request to its window and tray: the device's name,
   address and browser. It answers `{req, a_pub, nA}`.
3. The viewer sends `POST /api/pair/reveal {req, nV}`. The app checks the commit.
4. Both sides show `code = u32_be(H("wl-sas" || v_pub || a_pub || nV || nA)[0..4]) mod 10^6`.
   The owner compares the two codes and taps Allow (or Deny) on the computer.
5. Both derive `K = HKDF-SHA256(X25519, salt = H(v_pub || a_pub || nV || nA), info = "wilive-device-v1")`.
   The app stores `{id, name, K, added, last_seen}`, owner-only. The viewer keeps
   `{id, K}` in its local storage.

The commitment stops a man in the middle from grinding keys until the two codes
match. Pending requests are limited to 4 and expire after 2 minutes. A request that
is denied or ignored teaches the attacker nothing about K.

**Each session:** the app sends `{t:"chal", c}` (16 random bytes, single use). The
viewer answers `{t:"hello", id, mac = HMAC(K, "wl-hello|" + id + "|" + c)}`. Before the
proof is accepted, the app does only a JSON parse and one HMAC (HID-Fi's DoS rule).

**The owner stays in control:**

- The window and tray always show who is watching: device, address, display, sound,
  microphone, fps, rate and the latency estimate.
- One tap ends a viewer, forgets a device, or pauses all sharing.
- While anyone watches, the computer shows a red Live mark in the tray, so being
  watched is never silent.
- An access log is kept: pairings, sessions, refusals and ends. It is bounded and
  stays on this computer.

**Plain http for now (D-05).** A home device has no certificate a browser trusts, and
a self-signed one shows a warning page, which would be putting the problem on the
user. So the dashboard is served over http. The media is still encrypted (DTLS-SRTP),
and the page refuses an answer whose fingerprint differs from the one the app gave
it over the paired channel. The crypto in the page is pure JS (X25519, SHA-256,
HMAC, HKDF), checked against the RFC vectors, because WebCrypto is not available on
http. What http costs, stated plainly: an attacker who can rewrite traffic on the
home network could replace the page itself. The next step is trusted https: a
per-install name under a domain we own, with a real certificate (the Plex approach).
That also unlocks the viewer's own microphone (talk back), since `getUserMedia`
needs a secure page.

## The app

- **Workspace** (`crates/`):
  - `wilive-core`: devices, pairing crypto, the access log, settings. Pure, no OS
    code, and fully unit-tested.
  - `wilive-stream`: capture, encode, sound, WebRTC (fork of `hidfi-stream`).
  - `wilive-server`: one port for everything: the dashboard's files, the pairing
    API and the signaling WebSocket. Private-network peers only, bounded everything.
  - `wilive`: the tray, the window and the service lifecycle.
- **One UI codebase.** The dashboard has a viewer mode (watch) and an owner mode
  (who is watching, devices, log, settings). The owner window is the same page in a
  web view, given an owner token at start. A browser on the same computer is not
  the owner by being local.
- **Idle costs nothing.** With no viewer there is no capture and no encoder. With a
  viewer and a still screen, only damage-driven frames are sent.
- **Installers and updates** follow HID-Fi's release flow: a signed macOS app, a
  static-CRT Windows exe, a Linux build, one SHA256SUMS, and a signed update
  manifest. Built in CI on all four platforms.

## Brand

See `docs/BRAND.md`: red and warm whites, Inter and JetBrains Mono shipped inside
the app (no font CDN: the dashboard must work offline), and the logo in `brand/`.
