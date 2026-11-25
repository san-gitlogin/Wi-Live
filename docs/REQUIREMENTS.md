# Wi-Live: requirements

Each requirement has an ID, a status and how it is verified. Never mark one done
without naming the test or the check that proves it. Status: `todo`, `wip`, `done`.

Owner's brief (2026-11-25): "a solid one, firm, blazingly faster ... solid information
displayed on who's accessing, security aspects, audio, microphone, video sharing. I
can't tolerate any sort of lag: absolutely 0 latency and p99 ... all features, as
well as all on how we release HID-Fi, and follow all best practices, including test
automation ... screen sizes, clarity, multiple displays ... the dashboard that the
computer hosts in its home network, ultra blazing fast, handling all sorts of
exceptions, tested to the best core."

## Speed

| ID | Requirement | Status | Verified by |
|---|---|---|---|
| R-01 | Capture-to-present latency measured on every e2e run, with p50/p95/p99 reported and gated (DESIGN "Latency") | todo | tests/latency |
| R-02 | 60 fps by default where the hardware encoder keeps up; the rate falls before the latency rises | todo | |
| R-03 | Nothing queues: newest frame only, from capture to the browser | todo | stream unit tests |
| R-04 | First frame within 1 s of pairing a device or reopening the page | todo | e2e |
| R-05 | Dashboard: first paint under 100 ms on the LAN, no external requests, under 150 KB | todo | tests/ui perf |

## Picture, sound, displays

| ID | Requirement | Status | Verified by |
|---|---|---|---|
| R-10 | Every display can be watched; each viewer chooses its own; a display unplugged falls back to the main one, and the viewer is told | todo | |
| R-11 | Native-resolution clarity: text stays sharp. Size follows the viewer's screen and the link | todo | |
| R-12 | The computer's sound and its microphone, each on or off, shown to the viewer | todo | |
| R-13 | Viewer fit, fill, 1:1 and zoom/pan; full screen; phone either way up | todo | ui scenarios |
| R-14 | Talk back (the viewer's microphone) once trusted https exists (D-05) | todo | |

## Trust and control

| ID | Requirement | Status | Verified by |
|---|---|---|---|
| R-20 | Being on the network is never enough: only paired devices watch | todo | |
| R-21 | Pairing by numeric comparison with commitment; the owner allows on the computer | todo | core unit + e2e |
| R-22 | Each session proves the device key with a fresh challenge; replay fails | todo | |
| R-23 | Who is watching, live: device, address, display, sound, mic, fps, rate, latency | todo | |
| R-24 | End a viewer, forget a device, pause all sharing: each in one tap, effective at once | todo | |
| R-25 | A visible Live mark on the computer whenever anyone watches | todo | |
| R-26 | Access log: pairings, sessions, refusals, ends; bounded; never leaves the computer | todo | |
| R-27 | DoS bounds: private peers only, connection and request caps, body caps, bad-proof lockout | todo | server tests |
| R-28 | Media always encrypted (DTLS-SRTP) and the page checks the fingerprint | todo | |
| R-29 | Secrets: keys owner-only on disk, never logged, never in test output | todo | static guard |

## App, release, quality

| ID | Requirement | Status | Verified by |
|---|---|---|---|
| R-40 | Tray + window on macOS, Windows, Linux; start at login; single instance | todo | |
| R-41 | Signed installers and signed updates, released exactly as HID-Fi releases | todo | |
| R-42 | One test command (`python tests/run.py`) with tiers; CI on all platforms | todo | |
| R-43 | Every behaviour has a test; planted bugs must be caught (mutation sets) | todo | |
| R-44 | Light and dark themes, both meeting WCAG AA contrast, checked by a test | todo | |
| R-45 | Idle costs nothing: no capture, no encoder, no CPU without a viewer | todo | |
