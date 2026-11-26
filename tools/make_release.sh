#!/usr/bin/env bash
# Puts a Wi-Live release together from CI's artifacts (docs/RELEASING.md):
#
#   gh run download <run> -R san-gitlogin/Wi-Live -D dist/raw
#   tools/make_release.sh 0.1.0 dist/raw            check, sign, checksum, verify
#   tools/make_release.sh 0.1.0 dist/raw --publish  ...then tag and publish on GitHub
#
# Every file is checked before anything is signed: the version is the one in
# Cargo.toml, the macOS app is signed with Wi-Live's certificate, the Windows .exe is
# a real PE file needing no C runtime DLL, the Linux binaries are ELF for their CPU.
# SHA256SUMS is signed with the release key (~/.wilive-release) and the signature is
# checked against the public key in tools/release_keys.json before publishing.
set -euo pipefail
ver="${1:?version}"; raw="${2:?CI artifact folder}"; publish="${3:-}"
root="$(cd "$(dirname "$0")/.." && pwd)"
key="${WILIVE_RELEASE_KEY_DIR:-$HOME/.wilive-release}/wilive-release.pem"
have="$(grep -m1 '^version' "$root/Cargo.toml" | cut -d'"' -f2)"
[ "$have" = "$ver" ] || { echo "Cargo.toml says $have, not $ver"; exit 1; }
out="$root/dist/v$ver"; rm -rf "$out"; mkdir -p "$out"
find "$raw" -type f \( -name 'Wi-Live-*-macos.dmg' -o -name 'wilive-windows-*.exe' -o -name 'wilive-linux-*' \) -exec cp {} "$out/" \;
cd "$out"
ls Wi-Live-$ver-macos.dmg wilive-windows-x86_64.exe wilive-linux-x86_64 wilive-linux-aarch64 >/dev/null
"$root/tools/sign_mac_release.sh" "$out/Wi-Live-$ver-macos.dmg"
"$root/tools/sign_mac_release.sh" --check "$out/Wi-Live-$ver-macos.dmg"
python3 - <<'PY'
import pathlib, re, struct, sys
bad = []
d = pathlib.Path("wilive-windows-x86_64.exe").read_bytes()
pe = struct.unpack_from("<I", d, 0x3c)[0]
if d[:2] != b"MZ" or d[pe:pe + 4] != b"PE\0\0": bad.append("windows exe is not PE")
if re.search(rb"(?i)vcruntime140|msvcp140|api-ms-win-crt-", d): bad.append("windows exe needs a C runtime DLL")
for name, machine in (("wilive-linux-x86_64", 62), ("wilive-linux-aarch64", 183)):
    e = pathlib.Path(name).read_bytes()
    if e[:4] != b"\x7fELF" or struct.unpack_from("<H", e, 18)[0] != machine: bad.append(f"{name}: not ELF for its CPU")
# No shipped binary may trust the published TEST update key (a test build).
from cryptography.hazmat.primitives.asymmetric import ec
from cryptography.hazmat.primitives import serialization
scalar = bytes(0x57 ^ ((i * 29) & 0xFF) for i in range(32))
test_pub = ec.derive_private_key(int.from_bytes(scalar, "big"), ec.SECP256R1()).public_key().public_bytes(
    serialization.Encoding.X962, serialization.PublicFormat.UncompressedPoint).hex().encode()
import subprocess, tempfile, os
blobs = {n: pathlib.Path(n).read_bytes() for n in ("wilive-windows-x86_64.exe", "wilive-linux-x86_64", "wilive-linux-aarch64")}
m = tempfile.mkdtemp(); dmg = next(pathlib.Path(".").glob("Wi-Live-*-macos.dmg"))
subprocess.run(["hdiutil", "attach", "-quiet", "-nobrowse", "-readonly", "-mountpoint", m, str(dmg)], check=True)
try:
    blobs["macOS app"] = pathlib.Path(m, "Wi-Live.app", "Contents", "MacOS", "wilive").read_bytes()
finally:
    subprocess.run(["hdiutil", "detach", "-quiet", m])
release_key = b"0496d2da368e7422031cd01f0e70b4797e416800ce0f82d441ce2db16ea1e623e93eb34322abc16b51313e9fcdb18b3d74ce55edf1a4f0b74f7b30d1c359d2289c"
for n, b in blobs.items():
    if test_pub in b or b"wilive:test-key-build:never-ship" in b: bad.append(f"{n} trusts the TEST update key (a test build)")
    if release_key not in b: bad.append(f"{n} does not carry the release key")
print("checked: windows PE without C runtime, linux ELF x86_64 + aarch64, release key in every app, no TEST key")
sys.exit("; ".join(bad) if bad else 0)
PY
chmod +x wilive-linux-*
shasum -a 256 Wi-Live-$ver-macos.dmg wilive-windows-x86_64.exe wilive-linux-x86_64 wilive-linux-aarch64 > SHA256SUMS
openssl dgst -sha256 -sign "$key" -out SHA256SUMS.sig SHA256SUMS
python3 -c "import json,sys;open(sys.argv[2],'w').write(json.load(open(sys.argv[1]))['release']['public_pem'])" "$root/tools/release_keys.json" release.pub.pem
openssl dgst -sha256 -verify release.pub.pem -signature SHA256SUMS.sig SHA256SUMS
shasum -a 256 -c SHA256SUMS
echo "release v$ver ready in $out"
if [ "$publish" = "--publish" ]; then
  cd "$root"
  git tag -a "v$ver" -m "Wi-Live $ver" 2>/dev/null || true
  git push -q origin "v$ver"
  notes="$(awk "/^## v$ver/{f=1;next} /^## v/{f=0} f" CHANGELOG.md)"
  gh release create "v$ver" -R san-gitlogin/Wi-Live --title "Wi-Live $ver" --notes "$notes" ${WILIVE_PRERELEASE:+--prerelease} \
    "$out"/Wi-Live-$ver-macos.dmg "$out"/wilive-windows-x86_64.exe "$out"/wilive-linux-x86_64 "$out"/wilive-linux-aarch64 \
    "$out"/SHA256SUMS "$out"/SHA256SUMS.sig "$out"/release.pub.pem
fi
