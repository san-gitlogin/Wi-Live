<p align="center"><img src="brand/logo.svg" width="96" alt="Wi-Live"></p>

# Wi-Live

Your computer's screen, sound and microphone, live on any phone, tablet, TV or
computer in your home network. Open an address (or scan the QR code), compare a code
once, and watch. No account, no cloud, nothing to install on the watching device.

- **Fast.** Hardware H.264 over WebRTC, every frame shown the moment it is decoded.
  Capture to screen at 1080p60 measures p50 19-20 ms, p99 27-28.5 ms on this machine,
  across every display phase (tests/latency; docs/DECISIONS.md D-07).
- **Private.** Only devices you allow can watch: a six-digit code compared once and
  allowed on the computer. Every session proves the device's key, and the page checks
  the media is really your computer's. A red live mark in the menu bar while anyone
  watches; end a viewer, remove a device or pause everything in one tap.
- **Every display, your way.** One display or all of them at once, as screens shaped
  like the real ones (grid, row, column, focus; drag to arrange). Pinch and keyboard
  zoom, picture quality (Auto, Sharp, Smooth, Saver), picture in picture, snapshots.
- **Every screen.** Phones either way up (clear of the notch), tablets, TVs and big
  monitors, light and dark. Browsers back to iOS 12.2 Safari and Chrome 70.

## Install

From [Releases](https://github.com/san-gitlogin/Wi-Live/releases):

| | |
|---|---|
| macOS 12.3+ | `Wi-Live-<version>-macos.dmg`: drag to Applications, open, allow Screen Recording once |
| Windows 10/11 | `wilive-windows-x86_64.exe` |
| Linux (x86_64, ARM64) | `wilive-linux-<arch>`; it fetches nothing itself: run `package/fetch-openh264.sh` once for Cisco's H.264 encoder |

Check a download (docs/RELEASING.md):

```
shasum -a 256 -c SHA256SUMS --ignore-missing
openssl dgst -sha256 -verify release.pub.pem -signature SHA256SUMS.sig SHA256SUMS
```

## Build and test

```
cargo build --release -p wilive
python3 tests/run.py            # static + unit: fmt, clippy, Rust tests, the page's crypto
python3 tests/run.py --all      # + every browser tier in headless Chrome and WebKit
```

Windows and Linux are tested from a Mac in VMs (tests/windows, tests/linux) and in CI
on macOS, Windows and Linux x86_64 + ARM64. How it works: docs/DESIGN.md. Where it
stands: docs/PROGRESS.md.

Written in Rust: wilive-core (pairing, devices, log), wilive-stream (capture, encode,
WebRTC; from HID-Fi Companion), wilive-server (one port for everything), wilive (the
app). The dashboard is one small page with no framework.

Licence: PolyForm Noncommercial 1.0.0 (LICENSE.md). Fonts: Inter and JetBrains Mono,
SIL OFL 1.1.
