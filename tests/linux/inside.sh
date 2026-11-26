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

step "the app on a desktop session: its tray item, and the owner's window through a one-time link"
H=$(mktemp -d)
cat > /tmp/session.sh <<'SH'
WILIVE_HOME=$1 WILIVE_NO_AUTOSTART=1 ./target/release/wilive >/tmp/app.out 2>&1 &
APP=$!
sleep 6
kill -0 $APP 2>/dev/null || { echo "FAIL the app exited"; cat /tmp/app.out; exit 1; }
echo "ok   it starts and keeps running: $(ps -o rss= -p $APP | awk '{print int($1/1024)}') MB"
ITEM=$(dbus-send --session --dest=org.freedesktop.DBus --type=method_call --print-reply /org/freedesktop/DBus org.freedesktop.DBus.ListNames | grep -o 'org.kde.StatusNotifierItem[^"]*' | head -1)
if [ -n "$ITEM" ]; then echo "ok   tray item on the session bus: $ITEM"; else grep -q "no tray on this desktop" $1/wilive.log && echo "ok   no tray host here: said so and kept serving"; fi
LINK=$(grep -o 'http://127.0.0.1:[0-9]*/owner?code=[0-9a-f]*' /tmp/app.out | head -1)
[ -n "$LINK" ] || { echo "FAIL no owner link (first run opens the window; no browser here, so it is printed)"; cat /tmp/app.out; exit 1; }
CODE=$(curl -s -o /dev/null -w '%{http_code}' -c /tmp/ck "$LINK")
[ "$CODE" = 302 ] && echo "ok   the one-time link signs in (302 and a cookie)" || { echo "FAIL link answered $CODE"; exit 1; }
OK=$(curl -s -o /dev/null -w '%{http_code}' -b /tmp/ck http://127.0.0.1:47900/api/owner/state)
[ "$OK" = 200 ] && echo "ok   the owner's state with that session" || { echo "FAIL owner state $OK"; exit 1; }
AGAIN=$(curl -s -o /dev/null -w '%{http_code}' "$LINK")
[ "$AGAIN" = 403 ] && echo "ok   the link works once" || { echo "FAIL a used link answered $AGAIN"; exit 1; }
kill $APP
SH
dbus-run-session -- sh /tmp/session.sh "$H"

step "real X11 capture + OpenH264, paired and watched by a Python WebRTC viewer"
sh tests/linux/x11_e2e.sh ./target/release/examples/serve
echo; echo "ALL PASSED on Linux $(uname -m)"
