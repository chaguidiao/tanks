# Helpers for interactive guide screenshots: the client on Xvfb, driven by xdotool.
# Sourced by docs/guide/shots-*.sh inside the localhost/tanks-shots image (tanks-dev + xdotool + scrot).
export HOME=/tmp/guidehome DISPLAY=:99
mkdir -p $HOME out/guide/img
C=target/debug/client
O=out/guide/img
if ! xdpyinfo >/dev/null 2>&1; then Xvfb :99 -screen 0 1600x900x24 >/dev/null 2>&1 & sleep 1.5; fi
PID=
start() { $C "$@" >/tmp/client.log 2>&1 & PID=$!; sleep "${WAIT:-14}"; xdotool mousemove 800 450; xdotool key shift; sleep 0.4; }
stop() { kill $PID 2>/dev/null; wait $PID 2>/dev/null; PID=; }
snap() { sleep "${1:-0.6}"; scrot -o $O/$NAME.png; echo "shot $NAME"; }
mv_() { xdotool mousemove "$1" "$2"; sleep 0.3; }
lclick() { xdotool mousemove "$1" "$2"; sleep 0.15; xdotool mousedown 1; sleep 0.12; xdotool mouseup 1; sleep 0.4; }
rclick() { xdotool mousemove "$1" "$2"; sleep 0.15; xdotool mousedown 3; sleep 0.12; xdotool mouseup 3; sleep 0.4; }
rhold() { xdotool mousemove "$1" "$2"; sleep 0.15; xdotool mousedown 3; sleep 0.6; xdotool mouseup 3; sleep 0.4; }
key() { for k in "$@"; do xdotool key "$k"; sleep 0.25; done; }
