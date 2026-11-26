# Releasing Wi-Live

The same discipline as HID-Fi: nothing is published that was not built by CI on its
own OS, checked file by file, and signed with keys that never enter the repository.

## The keys (once, on the owner's Mac)

`~/.wilive-release` (override with `WILIVE_RELEASE_KEY_DIR`), owner-only:

| file | what |
|---|---|
| `wilive-release.pem` | ECDSA P-256 release key: signs every release's SHA256SUMS |
| `wilive-macos-signing.keychain-db` + `.pw` | the macOS certificate "Wi-Live Release (macOS)", in its own keychain |
| `wilive-macos-signing.p12` + `wilive-macos-p12.pw` | a backup of that certificate |

Their public halves are in `tools/release_keys.json`. **Back the folder up**
(an encrypted disk image, with HID-Fi's keys): losing the release key means a new
key, and every installed copy would have to be told about it by hand. Losing the
macOS certificate means every Mac asks for Screen Recording again after the next update.

Why a certificate of our own on macOS: CI signs the app ad hoc, which macOS knows only
by its hash, so every update would look like a new app and lose its permissions.
Signed with one certificate, the app is "live.wilive.app, certificate root <this one>"
in every version.

## A release

1. Bump `version` in `Cargo.toml`, add `## vX.Y.Z - date` to `CHANGELOG.md`, run
   `python3 tests/run.py --all`, commit and push.
2. `gh workflow run ci -R san-gitlogin/Wi-Live` and wait for all four jobs (macOS,
   Windows, Linux x86_64 and ARM64).
3. `gh run download <run id> -R san-gitlogin/Wi-Live -D dist/raw`
4. `tools/make_release.sh X.Y.Z dist/raw` - checks every file (the version, the macOS
   signature, the Windows .exe needs no C runtime, ELF for each Linux CPU), signs the
   macOS app, writes SHA256SUMS, signs it and verifies the signature against the public key.
5. `tools/make_release.sh X.Y.Z dist/raw --publish` - tags and publishes. Publish as a
   full release: GitHub's `releases/latest`, which the app's updater reads, skips
   pre-releases (`WILIVE_PRERELEASE=1` makes one, for a build nobody should update to).
   The script refuses any binary built with the feature `test-key` (it carries the marker
   `wilive:test-key-build:never-ship`) or missing the release key.
5b. Before publishing: `python3 tests/real_latency.py 12` with the new build installed
   (a clock window on screen, the probe device allowed once) - real-screen latency, by hand.
6. Download the release back and check it: `shasum -a 256 -c SHA256SUMS` and
   `openssl dgst -sha256 -verify release.pub.pem -signature SHA256SUMS.sig SHA256SUMS`.

## Checking a download (for anyone)

```
shasum -a 256 -c SHA256SUMS --ignore-missing
openssl dgst -sha256 -verify release.pub.pem -signature SHA256SUMS.sig SHA256SUMS
```
Compare `release.pub.pem` with `tools/release_keys.json` in the repository.
