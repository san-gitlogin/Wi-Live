# Changelog

## v0.1.0 - 2026-11-26

The first public build. Your computer's screen, sound and microphone on any phone,
tablet or computer in your home network, in its browser.

- **Fast.** Hardware H.264 over WebRTC; every frame shown the moment it is decoded
  (playout delay 0). Capture to screen on this machine, measured across every display
  phase: 1080p60 p50 19-20 ms, p99 27-28.5 ms (tests/latency).
- **Private.** Only devices you allow watch: pair once by comparing a six-digit code,
  allow on the computer. Every session proves the device's key; the page checks the
  media is your computer's. Live mark in the menu bar while anyone watches. End, remove,
  pause: one tap each. Activity log on the computer only.
- **Watch it your way.** Every display, or all at once as screens shaped like the real
  ones (grid, row, column, focus; drag to arrange). Pinch, scroll and keyboard zoom.
  Picture: Auto, Sharp, Smooth or Saver. Picture in picture, snapshots, shortcuts.
- **Phones.** QR code to open; remembered after the first time; iPhone full screen;
  Add to Home Screen; controls clear of the notch. Works back to iOS 12.2 Safari.
- **Tested** on macOS, Windows 11 (VM) and Linux ARM64 (VM), and in CI on macOS,
  Windows and Linux: unit, security, tampering, phones (WebKit), every screen size,
  endurance (memory trend, restarts, network stalls) and latency.

Known: the page is plain http (media encrypted; trusted https is next, and with it
talk back). Linux has no tray yet (it serves and prints its address). No in-app
updates yet: download new versions from Releases.
