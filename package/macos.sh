#!/usr/bin/env bash
# Builds "Wi-Live.app" and a .dmg to drag it from, for this Mac's architecture
# (or both with --universal). Ported from HID-Fi Companion's package/macos.sh.
#
# Signing: --sign "<identity>" for a release (HID-Fi signs with its own long-lived
# certificate, see its release notes; Wi-Live will get its own). Without it, a local
# "Wi-Live Local" identity is used if this Mac has one: macOS ties the Screen
# Recording permission to the signature, and an ad-hoc signature is a new identity on
# every build, so the permission would have to be granted again after each rebuild.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/.." && pwd)"
sign="-"; universal=0
while [ $# -gt 0 ]; do
  case "$1" in
    --sign) sign="$2"; shift 2;;
    --universal) universal=1; shift;;
    *) echo "unknown option $1" >&2; exit 2;;
  esac
done
ver="$(grep -m1 '^version' "$root/Cargo.toml" | cut -d'"' -f2)"
out="$root/target/package"
app="$out/Wi-Live.app"
rm -rf "$app"; mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cd "$root"
if [ "$universal" = 1 ]; then
  rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
  cargo build --release -p wilive --target aarch64-apple-darwin
  cargo build --release -p wilive --target x86_64-apple-darwin
  lipo -create -output "$app/Contents/MacOS/wilive" target/aarch64-apple-darwin/release/wilive target/x86_64-apple-darwin/release/wilive
else
  cargo build --release -p wilive
  cp target/release/wilive "$app/Contents/MacOS/wilive"
fi
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Wi-Live</string>
  <key>CFBundleDisplayName</key><string>Wi-Live</string>
  <key>CFBundleIdentifier</key><string>live.wilive.app</string>
  <key>CFBundleExecutable</key><string>wilive</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$ver</string>
  <key>CFBundleVersion</key><string>$ver</string>
  <key>LSMinimumSystemVersion</key><string>12.3</string>
  <key>LSUIElement</key><true/>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSHumanReadableCopyright</key><string>PolyForm Noncommercial 1.0.0</string>
  <key>NSLocalNetworkUsageDescription</key><string>Wi-Live shows this computer's screen to the devices you allow on your home network.</string>
  <key>NSAudioCaptureUsageDescription</key><string>Wi-Live shares this computer's sound with the devices watching it, when you switch sound sharing on.</string>
  <key>NSMicrophoneUsageDescription</key><string>Wi-Live shares this computer's microphone with the devices watching it, only when you switch "Share the microphone" on.</string>
</dict>
</plist>
PLIST
iconset="$out/AppIcon.iconset"; rm -rf "$iconset"; mkdir -p "$iconset"
src="$root/crates/wilive/assets/app-1024.png"
for s in 16 32 128 256 512; do
  sips -z $s $s "$src" --out "$iconset/icon_${s}x${s}.png" >/dev/null
  sips -z $((s*2)) $((s*2)) "$src" --out "$iconset/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/AppIcon.icns"
rm -rf "$iconset"
if [ "$sign" = "-" ] && security find-identity -p codesigning 2>/dev/null | grep -q '"Wi-Live Local"'; then
  codesign --force --sign "Wi-Live Local" --timestamp=none "$app"
elif [ "$sign" = "-" ]; then
  codesign --force --sign - --timestamp=none "$app"
else
  codesign --force --options runtime --timestamp --sign "$sign" "$app"
fi
dmg="$out/Wi-Live-$ver-macos.dmg"
stage="$out/dmg"; rm -rf "$stage" "$dmg"; mkdir -p "$stage"
cp -R "$app" "$stage/"
ln -s /Applications "$stage/Applications"
hdiutil create -quiet -volname "Wi-Live" -srcfolder "$stage" -ov -format UDZO "$dmg"
rm -rf "$stage"
echo "$app"
echo "$dmg"
