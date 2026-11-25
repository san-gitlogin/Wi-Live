#!/bin/sh
# Inside the container (see run.sh). The repository is at /src, read-only.
set -eu
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
step() { printf '\n== %s\n' "$*"; }
mkdir -p /work
tar -C /src --exclude=./target --exclude=./.git --exclude=./tests/.out -cf - . | tar -C /work -xf -
cd /work
step "build, unit tests and clippy, on Linux ($(uname -m))"
cargo test --workspace -q >/tmp/test.out 2>&1 || { echo "FAIL unit tests"; tail -40 /tmp/test.out; exit 1; }
grep -E "test result" /tmp/test.out | awk '{s+=$4} END {print "ok   unit tests pass on Linux: " s " tests"}'
cargo clippy -q --workspace --all-targets -- -D warnings && echo "ok   clippy -D warnings"
cargo build --release -q -p wilive -p wilive-server --bins --examples
echo "ok   release build: app $(du -h target/release/wilive | cut -f1)"

step "Cisco's OpenH264, as the installer fetches it"
LIB=$(sh package/fetch-openh264.sh "$HOME/.local/share/wilive") && echo "ok   $LIB"

step "the app itself (Linux has no tray yet: it serves and prints its address)"
H=$(mktemp -d)
WILIVE_HOME=$H WILIVE_NO_AUTOSTART=1 ./target/release/wilive >/tmp/app.out 2>&1 &
APP=$!
sleep 6
if kill -0 $APP 2>/dev/null; then
  echo "ok   it starts and keeps running: $(ps -o rss= -p $APP | awk '{print int($1/1024)}') MB"
  curl -sf http://127.0.0.1:47900/api/info && echo
else
  echo "FAIL the app exited"; cat /tmp/app.out; exit 1
fi
kill $APP

step "real X11 capture + OpenH264, paired and watched by a Python WebRTC viewer"
sh tests/linux/x11_e2e.sh ./target/release/examples/serve
echo; echo "ALL PASSED on Linux $(uname -m)"
