# Wi-Live: brand

Owner (2026-11-25): "different coloured theme than HID-Fi, maybe with red and shades
of white ... appropriate fonts and new logo that is not childish, but professional."

## The idea

Red is the colour of "on air": the recording light, the LIVE sign. Wi-Live uses red
for one meaning only: **live, and the one action that matters**. Everything else is
warm whites and near-blacks, so the red is never noise. A page with three red things
on it has lost the idea.

## Logo

- `brand/logo.svg` (app icon, site, installers): an open screen frame, a "W" inside,
  and the live dot in the frame's gap. The dot over the W also reads as "Wi".
- `brand/tray.svg` (tray and menu bar, 16 to 22 px): the frame and the dot only,
  because the W turns to mud below 24 px. On macOS it is a template image, and the dot
  is red only while someone is watching. That is the Live mark (R-25).
- Strokes are round-capped, 5.5 units on a 64 grid (4.5 for the W). Judge any
  change at 16, 24 and 48 px in both themes, not at design size.
- Never: gradients, a drop shadow, a second accent colour, or the dot in any colour
  but live red.

## Colour tokens

Both themes meet WCAG AA (4.5:1 for text, 3:1 for graphics). `tests/static`
recomputes every pair from `web/app.css`.

| token | light | dark | use |
|---|---|---|---|
| `--bg` | `#FAF7F5` | `#0F0C0C` | page |
| `--surface` | `#FFFFFF` | `#181313` | cards |
| `--surface-2` | `#F4EFEC` | `#221B1B` | inputs, hovers |
| `--line` | `#E7DFDB` | `#2E2626` | borders |
| `--text` | `#1A1414` | `#F6F1EF` | text |
| `--text-2` | `#5C504E` | `#B5A8A5` | secondary text |
| `--accent` | `#C41A27` | `#CF2733` | primary buttons (white text) |
| `--accent-text` | `#C41A27` | `#FF6B73` | red text and links |
| `--live` | `#E0202E` | `#FF3B47` | the live dot, the Live badge |
| `--ok` | `#16794A` | `#3CC488` | connected, allowed |
| `--warn` | `#9A5B00` | `#E0A040` | slow link, attention |

Errors use the accent red, always with an icon and words, never red alone.

## Type

- **Inter** (variable, Latin) for everything. 15 px body, 600 for headings, tabular
  numbers (`font-variant-numeric: tabular-nums`) wherever figures change live.
- **JetBrains Mono** for codes, addresses and fingerprints: the six-digit pairing
  code, IPs, ms figures.
- Both are shipped inside the app (`web/fonts`, SIL OFL 1.1, licences beside them).
  The dashboard never loads anything from the internet.

## Voice

Plain words a stranger understands. Say what happened and what to do: "Pixel 7 is
watching your main display", not "Session established". No exclamation marks.
