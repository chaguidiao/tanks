#!/usr/bin/env bash
# Menu screenshots for the user guide. Run inside the dev container from the repo root:
#   tools/dev/devbox.sh run bash docs/guide/shots-menus.sh
set -u
C=target/debug/client
O=out/guide/img
mkdir -p $O
shot() { local name=$1; shift; xvfb-run -a -s "-screen 0 1600x900x24" $C "$@" --screenshot $O/$name.png >/dev/null 2>&1 || echo "FAILED $name"; }
shot title --screen title --frames 40
shot main --screen main
shot play --screen play --frames 10
shot play-cw --screen play --keys down,down,wait5 --frames 12
shot options --screen options
shot database --screen database --frames 8
shot database-ammo --screen database --keys right,wait3 --frames 10
shot database-terrain --screen database --keys right,right,wait3 --frames 10
shot database-doctrine --screen database --keys right,right,right,wait3 --frames 10
shot database-models --screen database --keys right,right,right,right,wait3 --frames 10
shot edit-terrain-list --screen edit-terrain --frames 10
true
