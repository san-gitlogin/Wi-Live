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

step "a virtual desktop with two monitors"
Xvfb :99 -screen 0 3200x1200x24 +extension RANDR >/tmp/xvfb.log 2>&1 &
export DISPLAY=:99
for i in 1 2 3 4 5 6 7 8 9 10; do xdpyinfo >/dev/null 2>&1 && break; sleep 0.5; done
export WILIVE_X11_MONITORS="LEFT=1920x1080+0+0,RIGHT=1280x1200+1920+0"
xsetroot -solid '#c00000'
xlogo -geometry 1280x1200+1920+0 -bg '#0000c0' -fg '#0000c0' &
( while :; do xsetroot -solid '#c00000'; sleep 0.05; xsetroot -solid '#c00010'; sleep 0.05; done ) &

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
WILIVE_REAL_SCREEN=1 ./target/release/examples/serve 1920 1080 30 >/tmp/serve.json 2>/tmp/serve.log &
SRV=$!
for i in $(seq 1 20); do [ -s /tmp/serve.json ] && break; sleep 0.5; done
python3 tests/linux/view_e2e.py /tmp/serve.json || { tail -30 /tmp/serve.log; kill $SRV; exit 1; }
kill $SRV
echo; echo "ALL PASSED on Linux $(uname -m)"
