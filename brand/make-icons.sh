#!/bin/sh
# Renders the tray and app icons from brand/*.svg into crates/wilive/assets.
# Needs rsvg-convert (librsvg). Judge the results at 16-24 px in both themes.
set -e
cd "$(dirname "$0")"
A=../crates/wilive/assets
mkdir -p $A
FRAME='M38 10H17a9 9 0 0 0-9 9v26a9 9 0 0 0 9 9h30a9 9 0 0 0 9-9V26'
svg() { # $1 frame colour, $2 dot (filled|ring), $3 dot colour
  if [ "$2" = ring ]; then DOT="<circle cx=\"51\" cy=\"13\" r=\"5.5\" fill=\"none\" stroke=\"$3\" stroke-width=\"3\"/>"; else DOT="<circle cx=\"51\" cy=\"13\" r=\"7\" fill=\"$3\"/>"; fi
  echo "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 64 64\"><path d=\"$FRAME\" fill=\"none\" stroke=\"$1\" stroke-width=\"5.5\" stroke-linecap=\"round\"/>$DOT</svg>"
}
# macOS menu bar: idle is a template (the system colours it); live keeps its red dot,
# so its frame is a grey that reads on light and dark bars alike.
svg '#000' ring '#000' | rsvg-convert -w 44 -h 44 -o $A/mac-idle-44.png
svg '#8E8E93' filled '#E0202E' | rsvg-convert -w 44 -h 44 -o $A/mac-live-44.png
# Windows and Linux trays.
svg '#8E8E93' ring '#8E8E93' | rsvg-convert -w 64 -h 64 -o $A/color-idle-64.png
svg '#8E8E93' filled '#E0202E' | rsvg-convert -w 64 -h 64 -o $A/color-live-64.png
# The app icon: the logo on a warm-white tile.
cat > /tmp/wilive-app.svg <<SVG
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024">
  <rect x="80" y="80" width="864" height="864" rx="196" fill="#FAF7F5"/>
  <g transform="translate(192 192) scale(10)">
    <path d="$FRAME" fill="none" stroke="#1A1414" stroke-width="5.5" stroke-linecap="round"/>
    <circle cx="51" cy="13" r="7" fill="#E0202E"/>
    <path d="M19 23l5.5 18 7.5-14 7.5 14 5.5-18" fill="none" stroke="#1A1414" stroke-width="4.5" stroke-linecap="round" stroke-linejoin="round"/>
  </g>
</svg>
SVG
cp /tmp/wilive-app.svg app-icon.svg
rsvg-convert -w 1024 -h 1024 app-icon.svg -o $A/app-1024.png
rsvg-convert -w 256 -h 256 app-icon.svg -o $A/app-256.png
echo "icons written to $A"
