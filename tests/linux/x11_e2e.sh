#!/bin/sh
# Real X11 capture + OpenH264, paired and watched by the Python WebRTC viewer, on a
# virtual two-monitor desktop (left red 1920x1080, right blue 1280x1200; the left
# flickers between two reds so frames keep coming - a still desktop sends almost
# nothing, by design). Used by tests/linux/inside.sh (the VM) and by CI on ARM64.
#   tests/linux/x11_e2e.sh <path to the serve example>
set -eu
SERVE="${1:?serve binary}"
here="$(cd "$(dirname "$0")" && pwd)"
Xvfb :99 -screen 0 3200x1200x24 +extension RANDR >/tmp/xvfb.log 2>&1 &
export DISPLAY=:99
for i in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do xdpyinfo >/dev/null 2>&1 && break; sleep 0.5; done
export WILIVE_X11_MONITORS="LEFT=1920x1080+0+0,RIGHT=1280x1200+1920+0"
xsetroot -solid '#c00000'
xlogo -geometry 1280x1200+1920+0 -bg '#0000c0' -fg '#0000c0' &
( while :; do xsetroot -solid '#c00000'; sleep 0.05; xsetroot -solid '#c00010'; sleep 0.05; done ) &
FLICKER=$!
sleep 1
WILIVE_REAL_SCREEN=1 "$SERVE" 1920 1080 30 >/tmp/serve.json 2>/tmp/serve.log &
SRV=$!
for i in $(seq 1 30); do [ -s /tmp/serve.json ] && break; sleep 0.5; done
rc=0
python3 "$here/view_e2e.py" /tmp/serve.json || rc=$?
[ $rc = 0 ] || tail -30 /tmp/serve.log
kill $SRV $FLICKER 2>/dev/null || true
exit $rc
