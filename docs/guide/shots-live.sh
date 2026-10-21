#!/usr/bin/env bash
# Interactive guide screenshots (mouse driven with xdotool; keys at start go through the
# client's own --keys so they are not lost while the map loads). Runs in the
# localhost/tanks-shots image (tanks-dev + xdotool + scrot), from the repo root:
#   podman run --rm -v $PWD:$PWD -v ~/.local/share/tanks-dev/rust:/opt/rust -w $PWD localhost/tanks-shots bash docs/guide/shots-live.sh [filter]
cd "$(dirname "$0")/../.."
. docs/guide/drive-lib.sh
F=${1:-}
want() { [[ -z $F || $1 == *$F* ]]; }
S="--scenario m5-hold-the-village"
V2="--zoom 2.0 --center 4700,4000"
# world -> screen for V2: sx = 800 + (x-4700)/2, sy = 450 - (y-4000)/2

if want x-tooltip; then start $S $V2; NAME=x-tooltip; mv_ 1150 735; snap 1; stop; fi
if want x-bp; then start $S $V2 --keys wait5,b; lclick 800 200; mv_ 960 60; NAME=x-bp-tool; snap; stop; fi
if want x-pl; then start $S $V2 --keys wait5,g,p; lclick 500 120; lclick 800 60; lclick 1150 150; mv_ 1240 330; NAME=x-pl-tool; snap; stop; fi
if want x-shift; then start $S $V2
  xdotool keydown shift; mv_ 640 380; rclick 640 380; mv_ 760 230; rclick 760 230; mv_ 1000 160; NAME=x-shift-append; snap 0.8
  xdotool keyup shift; sleep 0.8; NAME=x-shift-sent; snap; stop; fi
if want x-los; then start $S $V2 --keys wait5,l; lclick 640 520; lclick 1000 120; NAME=x-los; snap 1; stop; fi
if want x-viewshed; then start $S $V2 --keys wait5,v; lclick 760 560; NAME=x-viewshed; snap 2; stop; fi
if want x-halt; then start $S $V2 --order 1:5300,4900 --advance 25 --keys wait5,4,h; sleep 4; NAME=x-halted; snap 0.1; stop; fi
if want x-box; then start $S $V2; xdotool mousemove 660 560 mousedown 1; sleep 0.2; xdotool mousemove 760 610; sleep 0.2; xdotool mousemove 930 650; NAME=x-box; snap 0.5; xdotool mouseup 1; stop; fi
if want x-plan; then start $S $V2 --keys wait5,p; lclick 700 520; lclick 640 380; lclick 760 230; lclick 1000 160; mv_ 1100 300; NAME=x-plan-new; snap; stop; fi
if want x-ctxhold; then start $S $V2; xdotool mousemove 1000 300 mousedown 3; NAME=x-ctx-hold; snap 0.12; xdotool mouseup 3; stop; fi
if want x-fire; then start --scenario m7d-fire-support --zoom 3 --center 4800,4400 --keys wait5,enter,4,y; lclick 900 330; sleep 11.5; NAME=x-fire-splash; snap 0.1; sleep 3.6; NAME=x-fire-impact; snap 0.1; stop; fi
if want x-fire-tool; then start --scenario m7d-fire-support --zoom 3 --center 4800,4400 --keys wait5,y; mv_ 900 330; NAME=x-fire-tool; snap 0.3; stop; fi
if want x-esc; then start $S $V2 --keys wait5,esc; NAME=x-esc; snap; stop; fi
if want x-smoke; then start $S --bp 1:4750,4500,45 --bp 2:4300,4650,45 --advance 650 --zoom 1.6 --center 4550,5300 --select 1; sleep 0.5; NAME=x-battle; snap 0.1; stop; fi
true
