#!/usr/bin/env bash
# Scripted (CLI) game screenshots for the user guide. Inside the dev container, repo root:
#   tools/dev/devbox.sh run bash docs/guide/shots-cli.sh [name-filter]
set -u
C=target/debug/client
O=out/guide/img
mkdir -p $O
F=${1:-}
shot() { local name=$1; shift; [[ -n $F && $name != *$F* ]] && return; xvfb-run -a -s "-screen 0 1600x900x24" $C "$@" --screenshot $O/$name.png >/tmp/$name.log 2>&1 || { echo "FAILED $name"; tail -3 /tmp/$name.log; }; echo "shot $name"; }
S="--scenario m5-hold-the-village"
BPS="--bp 1:4750,4500,45 --bp 2:4300,4650,45 --pl -:4200,5700,5500,5500,6400,4600"
V="--zoom 2.4 --center 4900,4700"
shot g-planned $S $BPS $V
shot g-set $S $BPS --advance 150 $V
shot g-contact $S $BPS --advance 420 $V
shot g-fight $S $BPS --advance 600 $V
shot g-late $S $BPS --advance 900 --zoom 3.5 --center 5000,5000
true
# ---- batch 2
SEL="--select 1"
shot g-overview $S $BPS $V $SEL
for l in slope traffic cover; do shot g-layer-$l $S $V --layer $l; done
shot g-contacts-zoom $S $BPS --advance 660 --zoom 1.1 --center 4600,5800 --select 2
shot g-status-card $S $BPS --advance 690 --zoom 2.4 --center 4700,5000 --select 2-2
shot g-route-sent $S --order 1:4300,4000,4250,4500,4600,5000 --advance 2 --zoom 2.0 --center 4500,4400
shot g-route-moving $S --order 1:4300,4000,4250,4500,4600,5000 --advance 45 --zoom 2.0 --center 4500,4400
shot g-halted $S --order 1:4300,4000,4250,4500,4600,5000 --advance 45 --select 1 --press H --advance 10 --zoom 2.0 --center 4500,4400
shot g-list-formation $S $V $SEL --press F
shot g-list-speed $S $V $SEL --press S
shot g-list-technique $S $V $SEL --press T
shot g-list-weapons $S $V $SEL --press W
shot g-list-ammo $S $V $SEL --press Q
shot g-list-oncontact $S $V $SEL --keys wait3,c --frames 8
shot g-list-graphics $S $V $SEL --keys wait3,g --frames 8
shot g-timing $S $V $SEL --press K
shot g-ctx-ground $S $V $SEL --rclick 1000,300,0.6 --frames 14
shot g-ctx-ground-sub $S $V $SEL --rclick 1000,300,0.6 --keys wait2,down,right --frames 16
shot g-ctx-unit $S $V $SEL --rclick 762,575,0.1 --frames 14
P="--plan 1:@docs/plans/example-plan.ron"
PV="--zoom 1.65 --center 4300,4307"
shot g-plan-draft $S $P --plan-draft $PV
shot g-plan-cpmenu $S $P --plan-draft $PV --rclick 752,418,0.1 --frames 14
shot g-plan-cpmenu-wait $S $P --plan-draft $PV --rclick 752,418,0.1 --keys wait2,w --frames 16
shot g-plan-cpmenu-leg $S $P --plan-draft $PV --rclick 752,418,0.1 --keys wait2,l --frames 16
shot g-plan-sent $S $P --advance 5 $PV
shot g-plan-waiting $S $P --advance 240 $PV
shot g-plan-hammer $S $P --advance 240 --code-word HAMMER --advance 40 $PV
shot g-multi $S $V --select 1+2
shot g-help $S $V --keys wait3,f7 --frames 8
shot g-pause $S $V --pause
shot g-dbvehicle $S $V --select 1-2 --keys wait3,i --frames 10
shot g-aar $S $BPS --advance 720 --aar
shot g-scened --scenario m5-hold-the-village --scenario-editor --zoom 4 --center 5200,5200
shot g-scened-wx --scenario m7c-fog-at-dawn --scenario-editor --keys 2 --frames 20
shot g-scened-fires --scenario m7d-fire-support --scenario-editor --keys 3,pgdn,= --frames 20
shot g-cont-menu $S --zoom 2 --center 2000,2400 --pl -:1200,2400,2800,2400 --select 1 --press K,H --click 1430,217 --frames 30
shot g-terrained --map demo --edit --zoom 2.4 --center 4600,4400
shot g-night --scenario m7c-night-attack --zoom 3 --center 4900,4700
shot g-fog --scenario mod-fog-meeting --zoom 3 --center 4900,4700
shot g-uplands --scenario cw-village-defence
shot g-uplands-zoom --scenario cw-village-defence --zoom 2.4 --center 4300,3800
shot g-firesupport --scenario m7d-fire-support --zoom 6 --center 4000,4000
true
# ---- batch 3 (units on screen)
V2="--zoom 2.0 --center 4700,4000"
for k in F S T W Q; do shot h-list-$k $S $V2 $SEL --press $k; done
shot h-list-C $S $V2 $SEL --keys wait3,c --frames 8
shot h-timing $S $V2 $SEL --press K
shot h-ctx-ground $S $V2 $SEL --rclick 1000,250,0.6 --frames 14
shot h-ctx-ground-orders $S $V2 $SEL --rclick 1000,250,0.6 --keys wait2,up,right --frames 16
shot h-ctx-unit $S $V2 $SEL --rclick 730,590,0.1 --frames 14
shot h-multi $S $V2 --select 1+2
shot h-uplands $S --scenario cw-village-defence --zoom 2.4 --center 4000,3400
shot h-dusk --scenario mod-dusk-attack
true
# ---- tour
shot tour $S --plan 1:@docs/plans/example-plan.ron --bp 2:4750,4450,45 --pl -:4000,5400,5000,5300,6000,4700 --advance 230 --zoom 2.2 --center 4500,4500 --select 1
true
