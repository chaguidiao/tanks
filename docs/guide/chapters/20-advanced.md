<div class="part"><div class="pn">PART II</div><div class="pt">Advanced</div>
<p>Routes and checkpoint plans, code words, contingencies, the context menu, artillery, smoke and terrain analysis. Then a look under the bonnet: how detection, gunnery, armour and damage are modelled, and how to use the database, the after-action review and the editors.</p></div>

# Routes, drafts and the Shift key

## One control model

A platoon's orders *are* its plan: a list of checkpoints to pass, with optional instructions at each. There is one way to build that list, and two mouse gestures cover nearly everything:

- **Right click** *replaces* the platoon's orders with "move here".
- **Shift + right click** *appends* a checkpoint to them.

Holding <kbd>Shift</kbd> works like a temporary mode: while it is down, each right click adds a checkpoint to a **draft**, and when you let go the draft is sent as one radio order. <kbd>Shift</kbd>+<kbd>B</kbd> appends a battle position at the end of the route, and <kbd>Shift</kbd>+<kbd>Z</kbd> appends "fire smoke here".

## The draft strip, undo and Esc

Whenever a draft exists, a strip appears above the command card: `DRAFT · n CHANGES`, **SEND**, **REVERT**, **UNDO**, **REDO**.

| Key | Effect |
|---|---|
| <kbd>Enter</kbd> | Send the draft |
| <kbd>Ctrl</kbd>+<kbd>Z</kbd> / <kbd>Ctrl</kbd>+<kbd>Y</kbd> | Undo / redo a draft edit. After a right click replaced a plan, <kbd>Ctrl</kbd>+<kbd>Z</kbd> brings the old plan back as a draft |
| <kbd>Esc</kbd> | Step back *one* level: cancel a pending click, then the tool, then the timing, then the selection. **Esc never throws a draft away**; the draft waits until you return to it |
| **REVERT** | Drop the draft. It asks for a second press when the draft holds several edits |

Drafts belong to their platoon. If you select another platoon, the draft is parked and comes back when you reselect the first one.

## How a route is executed

The route crosses the radio once. The platoon then plans each leg separately (so it really visits every checkpoint), keeps its formation through the checkpoints, and after a contact drill resumes with only the checkpoints it hasn't passed yet. A checkpoint counts as reached when a mobile vehicle is within about 60 m of it, allowing for its place in the formation.

# Checkpoint plans

A **checkpoint plan** is a route in which every leg and every checkpoint can carry its own instructions. It turns "move here" into something like *"move in column at top speed to CP 1, then in wedge at 15 km/h to CP 2, occupy a hull-down position there facing north-east and wait for code word HAMMER; on HAMMER pop smoke and advance in line to CP 3; if you meet the enemy within 2 km on the way, fall back to CP 4 instead."*

The whole plan is one radio order. Everything it says to do later happens without further transmissions, which is why plans beat improvised orders in a fight.

## Building a plan

1. Select a platoon and press **PLAN** (<kbd>P</kbd>). A draft starts from the plan last sent, the running plan, or the current route.
2. **LMB** adds checkpoints at the end. Drag a checkpoint to move it, <kbd>Del</kbd> deletes the selected one, <kbd>,</kbd> and <kbd>.</kbd> step through them.
3. **RMB on a checkpoint** opens its menu; **RMB on a leg** inserts a checkpoint or sets the leg.
4. **SEND** (<kbd>Enter</kbd>) issues the plan.

::fig x-plan-new | crop=440,30,1160,650 | pins=1@700,520;2@760,229;3@1000,159;4@1440,195;5@1440,300;6@1450,671 | A new plan with four checkpoints. Each CP shows its ETA <span class="pinref">1</span>–<span class="pinref">3</span>; the panel lists the CPs <span class="pinref">4</span> and the selected CP's settings <span class="pinref">5</span> (all **KEEP** / **NONE** so far); the draft strip <span class="pinref">6</span> counts four changes.

## The checkpoint menu

::fig g-plan-cpmenu-wait | crop=600,30,820,650 | pins=1@744,449;2@744,469;3@744,489;4@744,509;5@744,529 | RMB on CP 2 of the example plan, with the WAIT submenu open.

| | Row | Contents |
|---|---|---|
| <span class="pinref">1</span> | **LEG TO CP** | Settings for the leg *arriving* at this CP: **SPEED**, **FORMATION**, **TECHNIQUE**, **WEAPONS**, **ON CONTACT**, **ON AMBUSH** (rockets or missiles from close range: **REVERSE OUT** by default, **DRIVE THROUGH**, or **HALT AND FIGHT**), **BUILT-UP** (tanks without dismounts **AVOID** villages and towns by default; **ENTER** drives through). **KEEP** = don't change. The card's **ON CONTACT** list holds ON AMBUSH and BUILT-UP too, for the platoon now. The settings take effect as the lead section passes the previous CP |
| <span class="pinref">2</span> | **AT CP** | Actions, in order, on arrival: **FACE…** (a bearing), **OCCUPY BP** (hull-down or turret-down, facing a direction; the plan holds until the BP is reported set), **SMOKE SCREEN**, **ENGINE SMOKE ON/OFF**, **FIRE SMOKE AT…** (tank smoke rounds), **CALL FOR FIRE AT** (HE or smoke), **REPORT**; for a platoon of carriers **DISMOUNT** and **MOUNT** (the plan holds until the squads are out or aboard); for a dismount platoon only **OCCUPY BUILDING** (the house at or next to the CP), **OCCUPY BUILDINGS FACING…** (one squad to each house within 200 m that sees the direction you click), **CALL FOR FIRE AT** and **REPORT** |
| <span class="pinref">3</span> | **WAIT** | Hold at the CP until a condition is met (next table). A CP with no actions and no wait is a pass-through: the formation flows through it |
| <span class="pinref">4</span> | **ON RELEASE** | Actions to run when the wait ends, before the next leg: the classic "on HAMMER: smoke, fire, go" |
| <span class="pinref">5</span> | **IF ON LEG** | A **branch**: if a condition happens while on the leg to this CP or while waiting here, continue at another CP instead. **AMBUSHED** (rockets or missiles fired at the platoon from close range) is carried out at once, even in the middle of the ambush drill |

Below these: **MOVE CP…**, **INSERT CP BEFORE**, **DELETE CP**, **START HERE** (the plan begins at this CP when sent) and **CLEAR PLAN**. Items ending in "…" take one more map click (a bearing or a target point).

### Wait conditions

| Condition | Released when |
|---|---|
| **CODE WORD** ▸ HAMMER, ANVIL, SWORD … | You send that code word (after the platoon started waiting) |
| **FOR** ▸ 1–15 MIN | That much time has passed since arrival |
| **UNTIL** ▸ clock time | The scenario clock reaches the time |
| **ENEMY CROSSES** ▸ PL … | A fresh contact is reported beyond that phase line |
| **ENEMY IN** ▸ OBJ / BP / TRP | A fresh contact is inside that objective, battle position, or near that target reference point |
| **PLT REACHES** ▸ PLT *n* ▸ CP *m* | Another platoon reaches one of its checkpoints. Use it to sequence platoons ("2nd moves when 1st is at CP 3") |
| **NONE** | No wait |

### Branch conditions

**IF CONTACT < 1 / 2 / 3 KM**, **IF UNDER FIRE**, **IF STRENGTH <= 3 / 2**, each followed by **GO TO CP n**. On the map a branch is a dotted arrow to the target CP, labelled with its condition.

<div class="warn" markdown="1">
Every condition reads *your* picture: what your crews have reported, never the truth. "Enemy crosses PL RED" fires when a contact is *reported* beyond the line. A well-hidden enemy can cross without triggering anything.
</div>

## Reading a plan on the map

::fig g-plan-draft | crop=560,30,1040,650 | pins=1@890,60;2@750,417;3@708,471;4@818,647;5@790,229;6@860,272 | The example plan (`docs/plans/example-plan.ron`) as a draft.

| | |
|---|---|
| <span class="pinref">1</span> | CP 3 with its badges: ETA `?+1:31`, `BP HD 0800` (hull-down BP facing 0800 mils), `WAIT ANVIL` |
| <span class="pinref">2</span> | CP 2: `WAIT HAMMER`, `THEN SMOKE` (the on-release action) |
| <span class="pinref">3</span> | CP 4, the fall-back position reached only by the branch |
| <span class="pinref">4</span> | CP 1, `ETA 06:00` |
| <span class="pinref">5</span> | A mid-leg label: `25 KM/H · LINE · CONTACT CONTINUE`. Speed is also shown as tick marks across the leg |
| <span class="pinref">6</span> | The branch: `IF CONTACT < 2.0 KM` → CP 4 |

**Drafts** are dashed in the draft colour (amber), a **sent** plan is dashed until acknowledged, and a **running** plan is solid. **ETA** is estimated from the terrain and the leg's speed. After a wait whose end can't be predicted (a code word, an enemy movement), later ETAs read `?+m:ss`: *m:ss after that wait is released*. The ETA uses planning speeds over dry ground; in practice expect platoons to be somewhat slower, especially when forming up or in rain.

## Running plans

::fig g-plan-waiting | crop=0,30,1600,850 | pins=1@125,142;2@1440,240;3@545,812;4@1550,750 | The example plan at 06:04. 1st Platoon reached CP 2 and is occupying its battle position. The code-word strip <span class="pinref">1</span> shows HAMMER: `1 MOVING` (not set yet); the step list <span class="pinref">2</span> highlights CP 2; the radio <span class="pinref">3</span> reports `CP 2. WAITING HAMMER.`; the PLAN button <span class="pinref">4</span> shows progress (`CP 2/4`).

- **Editing a running plan:** press **PLAN** again. The draft starts from the running plan; when you send it, it keeps its progress (it starts at the current checkpoint).
- **Posture orders while a plan runs** (formation, speed, weapons, technique, on contact) change the leg in progress up to the next stop, so the next pass-through checkpoint doesn't undo them. Later legs keep their planned settings.
- **HALT** pauses the plan; **RESUME** continues the same step.
- **A new move, battle position or break-contact order replaces the plan.** The ghost line warns you (`REPLACES PLAN`), and <kbd>Ctrl</kbd>+<kbd>Z</kbd> brings the old plan back as a draft.

<div class="doctrine" markdown="1">
<span class="label">Realism note</span>
Real orders are mostly intent plus a few control measures, and subordinates use their initiative. A short plan with one or two contingencies is more realistic, and usually works better, than a long rigid script. The defaults leave the platoon AI free to react (contact drill on).
</div>

# Code words and synchronised action

A **code word** is a short signal agreed in advance. One transmission reaches every platoon at once and releases everything waiting on it. Code words let several platoons act at the same moment ("on HAMMER, everyone fires smoke and moves"), which is very hard to do with separate orders and their separate radio delays.

The doctrine pack provides the words: HAMMER, ANVIL, SWORD, SHIELD, FALCON, RAVEN, COBRA, TIGER, EAGLE.

## The code-word strip

As soon as a sent plan or contingency waits on a code word, the word appears in the **CODE WORDS** box under the order of battle, with each waiting platoon's state taken from its own reports:

| State | Meaning |
|---|---|
| `1 SET` | Platoon 1 is holding at the checkpoint that waits on the word (and its BP, if any, is set) |
| `1 MOVING` | On its way there, or still settling into its BP |
| `1 LATER` | Its wait on this word is at a later checkpoint |
| `ALL SET` | Everyone waiting is ready. With **PAUSE WHEN SET** on, the clock stops here so you can choose the moment |
| `1 CONTINGENCY` | A contingency order ({{ch:Control measures and contingencies}}) waits on it |

**Click a word** or press <kbd>Ctrl</kbd>+<kbd>1</kbd>–<kbd>9</kbd> (its place in the list: HAMMER = 1) to send it. The radio says `ALL STATIONS, CODE WORD HAMMER.` and `HAMMER: ALL SET.` when everyone is ready.

::fig g-plan-hammer | crop=0,30,1600,850 | pins=1@125,142;2@560,750 | 40 seconds after HAMMER. The platoon finished setting into its BP, fired its smoke salvo (radio <span class="pinref">2</span>: `SMOKE LAUNCHED` ×4) and is advancing in line to CP 3. Only ANVIL is left in the strip <span class="pinref">1</span>.

<div class="note" markdown="1">
A code word counts only if it is sent *after* the platoon started waiting. Sending HAMMER early does nothing for a platoon that is still on its way. In this version a code word always arrives and is never missed; communications problems may come later.
</div>

# Control measures and contingencies

## Graphics: phase lines, objectives and TRPs

**GRAPHICS** (<kbd>G</kbd>) opens a list: **PHASE LINE**, **OBJECTIVE**, **TRP**. Graphics are shared with all your platoons at once and need no radio time.

| Graphic | How to draw | What it is for |
|---|---|---|
| **Phase line** (PL RED, PL BLUE …) | LMB for each point, <kbd>Enter</kbd> or RMB to finish, <kbd>Backspace</kbd> removes the last point | A named line to report against (`300 M BEYOND PL RED`) and to trigger on (enemy crosses) |
| **Objective** (OBJ A, OBJ B …) | LMB the centre, LMB again for the radius | An area to attack or watch; plan waits can trigger on "enemy in OBJ A" |
| **TRP** (TRP 1, 2 …) | LMB once | A *target reference point*: a recognisable spot used to describe targets and to trigger on |

::fig x-pl-tool | crop=380,30,960,340 | w=80 | Drawing a phase line: three points placed, the fourth follows the mouse.

RMB on a graphic opens its menu: **MOVE TO** (orders the selection there), **CALL FOR FIRE ON**, **LOS FROM HERE**, **MOVE GRAPHIC** (the next left click puts it down there), **RADIUS +100 M** / **RADIUS -100 M** (objectives and battle positions), **TURN 45°** (battle positions), **DELETE**. A changed graphic keeps its name, and contingencies and plan waits that use it follow it.

## Contingency orders (TIMING)

**TIMING** (<kbd>K</kbd>) changes when the *next* order is carried out. It cycles:

| Value | The next order executes |
|---|---|
| **NOW** | Immediately (normal) |
| **ON CROSSING** | When an enemy is reported crossing the phase line drawn most recently |
| **+5 MIN** | Five minutes from now |

The setting applies to one order and then returns to NOW. The radio confirms `CONTINGENCY SET: ON ENEMY CROSSING PL RED.` and later `CONTINGENCY 1: EXECUTE …` when it fires. Because a contingency is sent before the event, it executes with no radio delay at the critical moment.

**Example:** a platoon in a forward battle position should fall back when Red reaches PL RED. Select it, press <kbd>K</kbd> until **ON CROSSING** shows, then right-click the fall-back position.

Contingencies that have not fired are listed at the bottom of the selection panel (`CONT 1: ENEMY X PL RED -> MOVE`). Click one to change it: **5 MIN LATER** / **5 MIN EARLIER** for a timed one, **PHASE LINE** ▸ another line for a crossing one, or **CANCEL**.

::fig g-cont-menu | crop=1270,30,330,260 | w=50 | A contingency's menu, opened from its row in the selection panel.

# The context menu

Right-click with a short press moves the selection. **Hold** the right button for a quarter of a second (a ring fills around the cursor), or right-click an *object*, and a menu opens instead. Its items do the same as the command card buttons, at the point you clicked.

::fig h-ctx-ground-orders | crop=880,200,720,460 | w=75 | The ground menu, with the ORDERS submenu open.

| Clicked on | Menu items |
|---|---|
| Ground | **MOVE HERE** (with the current speed), **MOVE HERE AT** ▸ speed, **ADD CP**, **BATTLE POSITION HERE**, **SMOKE ROUNDS HERE**, **CALL FOR FIRE** ▸ HE / smoke, **DRAW** ▸ phase line / objective / TRP, **LOS FROM HERE**, **VIEWSHED HERE**, **NEXT ORDER** ▸ timing, **ORDERS** ▸ halt, posture lists, smoke, reverse, ammunition, break contact |
| Own vehicle | **SELECT PLATOON**, **SELECT VEHICLE**, **DATA SHEET**, **EDIT PLAN**, **CLEAR PLAN**, **ORDERS** ▸ |
| Contact | **ENGAGE**, **CALL FOR FIRE ON**, **SMOKE ROUNDS ON**, **LOS TO** |
| Graphic | **MOVE TO**, **CALL FOR FIRE ON**, **LOS FROM HERE**, **MOVE GRAPHIC**, **RADIUS** ±100 M, **TURN 45°**, **DELETE** |
| Plan checkpoint / leg | The checkpoint menu ({{ch:Checkpoint plans}}) / **INSERT CP HERE** and the leg settings |

::fig h-ctx-unit | crop=600,540,400,160 | w=55 | The menu on an own vehicle.

Keyboard: <kbd>↑</kbd>/<kbd>↓</kbd> move, <kbd>Enter</kbd> or <kbd>→</kbd> open, <kbd>←</kbd> or <kbd>Esc</kbd> back, and the bracketed letters jump to an item. Greyed items say why they are unavailable (`NO BATTERY`, `NO SMOKE ROUNDS`, `NO PLATOON SELECTED`). With **PAUSE ON MENU** on, the clock stops while a menu is open: you get as much time as you need to decide.

# Commanding several platoons

Select several platoons with <kbd>Shift</kbd>+click or a box ({{ch:Selecting units}}). Moves, battle positions, posture changes, halt/resume, smoke, ammunition and break contact then go to each of them as separate radio orders. Destinations keep each platoon's offset from the first one selected, so a two-platoon move keeps its frontage.

::fig h-multi | crop=0,30,1600,850 | w=90 | Two platoons selected. The card title reads `PLATOON 1 + 2`.

Store a selection with <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>1</kbd>–<kbd>9</kbd> and recall it with <kbd>Shift</kbd>+<kbd>1</kbd>–<kbd>9</kbd>. Plans and engage orders are per platoon or vehicle.

# Infantry

Infantry is in the game to support and threaten tanks, not as a war of its own. A dismounted **squad** is one element on the map with a strength in men, a posture and its weapons. Squads riding in their carriers are simply part of the carrier: they are not seen, do not see, and share its fate (a penetration costs about a third of the men, a catastrophic kill all of them; survivors bail out).

## Squads and their carriers

IFVs (WARD M2, SKIF M1) carry squads; the roster lists the men aboard. The squads of a carrier platoon form a **dismount platoon** with the callsign `<platoon>-D` (for example `2-D`), which you select and order like any platoon.

- **DISMOUNT** (<kbd>U</kbd>, on the carrier platoon or its dismount platoon): the carriers halt for the ramp drill, and the squads get out and take security behind them. Order it short of the danger, not in it: the drill is counted only while the carriers stand still.
- **MOUNT** (<kbd>U</kbd> again): the squads walk back and board.
- A move or plan given to the dismount platoon while its squads still ride starts once they are out.

Squads move on foot: slowly, through woods and over ground no vehicle can cross, and into buildings only through their doors. A dismount platoon moves on line.

## Buildings

Select squads and press **OCCUPY BLDG** (<kbd>B</kbd>), then click a house, or simply right-click a house. The squads go in by its doors, as many as its floor area holds, at most four. Inside, they are seen and engaged only through the wall facing the enemy, from the upper floor, and they are protected by the building's class: a timber house stops little, a masonry house stops machine-gun fire and needs HE to clear. The plan action **OCCUPY BUILDINGS FACING...** lets the squads pick the houses that cover a direction themselves: masonry, upper floors and the forward houses first, one squad per house.

## Postures and fire

A squad is **moving**, **halted** or **dug in** (after a few minutes in place, or at once in a building). Each posture is smaller and harder to find than the last; tall crops in summer hide a halted squad too. Under fire a squad runs the **react to contact** drill: it stops, takes cover and continues once it is quiet.

Small arms rarely kill, but they **suppress**: a pinned squad stops moving and fires slower and worse. Your tanks use the coaxial machine gun on squads in the open and HE on squads behind walls. The squad card shows strength, posture, suppression and weapons.

## Anti-tank teams and ambushes

Enemy squads carry rocket launchers (RPG class), and missile teams carry guided missiles. A set-up squad waits until a vehicle comes inside its trigger range and prefers a flank shot. A **launch gives the shooter away**: a launch cloud shows for 3 s, and your crews treat a squad seen launching as their first target. A guided missile misses if its gunner is pinned during the flight, so firing back at once saves tanks.

A rocket or missile from a squad within 400 m is an **ambush**. The platoon's **ON AMBUSH** setting decides the drill (card under ON CONTACT, or per leg in a plan): **REVERSE OUT** (the default: smoke, reverse 150 m, then fight), **DRIVE THROUGH** (smoke and full speed out of the kill zone) or **HALT**. The plan condition **AMBUSHED** branches a plan when it happens.

## Built-up ground

Tank platoons on their own route round villages and towns, as doctrine says they should: the streets are where infantry kills tanks. A leg set to **BUILT-UP ENTER**, a carrier platoon in the group or friendly squads on the ground within 300 m lift the rule. To take a village, dismount the infantry short of it, let the squads clear the houses, and bring the tanks in behind them as a fire base.

# Fire support

Some scenarios give you an artillery battery, shown in the **FIRE SUPPORT** box (for example `ARTY 1 155 MM SP BATTERY · HE 72 SMK 16 READY`). The guns are kilometres away and not on the map; you call for fire over the radio like a real company commander.

## Calling for fire

1. Press **CALL FIRE** (<kbd>Y</kbd>). Press <kbd>Y</kbd> again to toggle between **HE** (high explosive) and **SMOKE**.
2. LMB the target point (or use the context menu: **CALL FOR FIRE** ▸ …).

The mission then runs like a real one:

| Radio | What is happening |
|---|---|
| `TO ARTY 1: FIRE MISSION, GRID 051 047, HE, FIRE FOR EFFECT, OVER.` | Your call (with the usual radio delay) |
| `ARTY 1: FIRE MISSION, GRID 051 047. SHOT IN 95 SECONDS, OVER.` | Accepted. The fire direction centre (FDC) computes the firing data: about 75 s for a Western battery, 90 s for Red |
| `ARTY 1: SHOT, OVER.` | The guns have fired; the shells are in the air (8 s plus about 3.6 s per km of range) |
| `ARTY 1: SPLASH, OVER.` | Five seconds before each volley lands |
| `ARTY 1: ROUNDS COMPLETE, END OF MISSION.` | Done. An HE mission is three volleys from four guns, 15 s apart; a smoke mission is one volley |

A mission can be refused: `UNABLE. OUT OF RANGE` / `BELOW MINIMUM RANGE` (155 mm: 2–22 km) / `NO AMMUNITION` / `MISSION IN PROGRESS` / `DISPLACING`.

::fig x-fire-impact | crop=700,240,500,200 | w=70 | The mission marker (`ARTY 1 HE`) with the first burst rings next to it, 1.2 km from your tanks.

## Accuracy and adjusting

Artillery is not precise. Every round scatters (the four guns' rounds also spread about 25 m apart), and an unadjusted battery has a **systematic error** too: its rounds fall consistently off target, by about half a per cent of the range plus 1.5–2 mils sideways. A mission at 4 km typically lands 15–20 m off, one at 15 km about 60 m off.

To correct it, watch where the bursts land, then press **ADJUST** (<kbd>J</kbd>) and click where they *should* have landed. The battery shifts its aim by the difference and fires two more volleys after only about 20 s.

<div class="warn" markdown="1">
**You only see bursts near your own troops.** Impacts appear on your map only within 1.5 km of one of your vehicles. ADJUST stays greyed until a burst has been observed, so a mission fired far beyond your front can't be adjusted.
</div>

## Ammunition and moves

A battery's rounds run out. When its HE drops below a quarter of what it started with, its ammunition vehicles drive back to the trains (`ARTY 1: AMMUNITION LOW. RESUPPLY IN 25 MINUTES.`) and return with a full load (`RESUPPLIED. 54 HE, 4 SMOKE.`); the guns keep firing what is left meanwhile. The trains hold two loads, then that is it. The FIRE SUPPORT box shows `RESUPPLY IN … MIN, … LEFT` while they are away.

After three missions from one position (the 152 mm battery: four) a battery moves to avoid counter-battery fire: `DISPLACING. READY IN 5 MINUTES.` It refuses calls until `IN POSITION, READY TO FIRE.` Plan your fires around it: spend missions on what matters, and don't call the third one just before the attack.

## What artillery does

HE wrecks light vehicles (infantry fighting vehicles, reconnaissance vehicles) and damages tanks only with direct or very close hits, but it **suppresses** everyone nearby. A suppressed crew searches worse (up to 50 % fewer detections) and aims and reloads more slowly (up to 80 % slower). Suppression fades in about half a minute once the fire stops. A platoon under artillery fire runs the **react to indirect fire** drill: if moving, it speeds up to clear the area; if stationary, it buttons up and holds.

Artillery **smoke** lays a large white-phosphorus screen (about 45 m across per round, lasting about 100 s) that blocks visible light.

The enemy's commander uses artillery the same way. Expect fire on positions it has reported.

# Smoke

Smoke blocks sight, and sight decides everything. The game models four sources:

| Source | Order | Size and duration | Blocks |
|---|---|---|---|
| Smoke grenades | **SMOKE** (<kbd>Z</kbd>); crews also fire them automatically within 2 s of coming under fire, at most every 20 s | A wall about 12–14 m deep in front of the vehicle, about a minute | Visible light and near-infrared. **Multispectral** grenades (from 1998) also block thermal sights |
| Engine smoke | **ENG SMOKE** (<kbd>E</kbd>), Red vehicles mostly | A thin trail, 30 s | Visible light |
| Tank smoke rounds | **SMOKE RND** (<kbd>N</kbd>) then LMB, or **SMOKE ROUNDS HERE** | A white-phosphorus shell, 20 m, 70 s; 1–2 rounds per tank across the line of fire | Visible light |
| Artillery smoke | **CALL FIRE** ▸ SMOKE | 45 m per round, 100 s | Visible light |

Smoke drifts with the scenario wind and thins out. On the map it appears as grey discs.

**Using smoke well:**

- Blind the enemy, not yourself. Put smoke rounds *on or just in front of* the enemy position, so they can't see you cross open ground.
- Thermal sights see through most smoke. Against a modern enemy with thermals, ordinary smoke hides you only from the enemy's eyes and day optics.
- Smoke draws attention and artillery. Move as soon as it builds up.
- **BREAK CONTACT** automatically fires smoke before withdrawing.

# Logistics

Tanks carry about forty main-gun rounds and fuel for a day; in a long fight both run out. The status card of a vehicle shows what is left (`MAIN GUN 12/40 RDS`), and an empty tank still fights with what else it has, or not at all.

## Supply points

Some scenarios give a side **supply points** (the `SP` pin on the map, with a dashed circle for the serving area when zoomed in). Select a platoon and press **RESUPPLY** (<kbd>R</kbd>): it drives to the nearest point, waits nearby, and its vehicles are served two at a time. Main-gun rounds go in at about three a minute and fuel at 200 litres a minute, so a platoon is out of the fight for a good quarter of an hour. The platoon card shows the progress (`RESUPPLY AT SP 1: 1 SERVED, 2 QUEUED, 1 DONE`), and the platoon reports `RESUPPLY COMPLETE.` and holds, or `RESUPPLY PARTIAL. SUPPLY POINT SHORT.` if the stock ran out. A new order interrupts it; what was already loaded stays loaded.

Resupply one platoon at a time, behind cover, while the others hold.

## LOGPACs

A scenario can also bring a **LOGPAC** (logistics package) at a set time: `LOGPAC RECEIVED. REARMED.` or `REFUELLED.` The rounds and fuel arrive at once, wherever the platoon is. The scenario editor's supply points (<kbd>S</kbd>) and the RON file's `logistics` list set both up.

# Terrain analysis

## Line of sight (L)

Press **LOS** (<kbd>L</kbd>), click the observer point, then the target point. A green line means clear; a red line means **NO LOS**, with the range and bearing. A height profile of the ground between the points appears near the bottom of the screen.

::fig x-los | crop=600,100,640,460 | w=70 | NO LOS over 1.1 km at 0746 mils: the village blocks the line.

The observer's eye height is the selected vehicle's sight height (about 2.3 m for a tank); the target height is 2.4 m (a tank's turret roof). Trees, buildings and smoke are taken into account.

## Viewshed (V)

Press **VIEWSHED** (<kbd>V</kbd>) and click a point: everything visible from there (up to 3 km, at the selected vehicle's sight height) is shown green, everything hidden red. <kbd>V</kbd> or <kbd>Esc</kbd> clears it.

::fig x-viewshed | crop=0,30,1270,660 | w=90 | Viewshed from just south of 1st Platoon: the village and the low rise north of it hide the ground beyond (red).

Use the viewshed during planning to check a battle position: *can my tanks see the enemy's approach?* Then turn it around: from where the enemy will come, *can the enemy see my tanks?* The best positions see far but are seen little.

## Hull-down, turret-down and other positions

A tank behind a gentle crest can choose how much of itself to show:

<div class="diagram"><svg viewBox="0 0 640 145" width="600" xmlns="http://www.w3.org/2000/svg" font-family="IBM Plex Mono"><g transform="translate(0,0)"><line x1="0" y1="100" x2="150" y2="100" stroke="#7a6f55" stroke-width="1.5"/><rect x="20" y="86" width="44" height="14" fill="#5b7393"/><rect x="30" y="76" width="22" height="10" fill="#5b7393"/><line x1="52" y1="81" x2="80" y2="81" stroke="#5b7393" stroke-width="3"/><rect x="34" y="71" width="5" height="5" fill="#5b7393"/><text x="75" y="122" text-anchor="middle" font-size="11">EXPOSED</text><text x="75" y="136" text-anchor="middle" font-size="8.5" fill="#555">whole vehicle visible</text></g><g transform="translate(163,0)"><line x1="0" y1="100" x2="150" y2="100" stroke="#7a6f55" stroke-width="1.5"/><path d="M78,100 C88,86 100,86 112,86 C126,86 134,100 150,100 Z" fill="#d9d2c0" stroke="#7a6f55" stroke-width="1.2"/><rect x="20" y="86" width="44" height="14" fill="#5b7393"/><rect x="30" y="76" width="22" height="10" fill="#5b7393"/><line x1="52" y1="81" x2="80" y2="81" stroke="#5b7393" stroke-width="3"/><rect x="34" y="71" width="5" height="5" fill="#5b7393"/><line x1="150" y1="83" x2="0" y2="87" stroke="#a33" stroke-width="1" stroke-dasharray="4,3"/><text x="75" y="122" text-anchor="middle" font-size="11">HULL-DOWN</text><text x="75" y="136" text-anchor="middle" font-size="8.5" fill="#555">hull hidden, gun clear</text></g><g transform="translate(326,0)"><line x1="0" y1="100" x2="150" y2="100" stroke="#7a6f55" stroke-width="1.5"/><path d="M78,100 C88,73 100,73 112,73 C126,73 134,100 150,100 Z" fill="#d9d2c0" stroke="#7a6f55" stroke-width="1.2"/><rect x="20" y="86" width="44" height="14" fill="#5b7393"/><rect x="30" y="76" width="22" height="10" fill="#5b7393"/><line x1="52" y1="81" x2="80" y2="81" stroke="#5b7393" stroke-width="3"/><rect x="34" y="71" width="5" height="5" fill="#5b7393"/><line x1="150" y1="70" x2="0" y2="74" stroke="#a33" stroke-width="1" stroke-dasharray="4,3"/><text x="75" y="122" text-anchor="middle" font-size="11">TURRET-DOWN</text><text x="75" y="136" text-anchor="middle" font-size="8.5" fill="#555">only the sight looks over</text></g><g transform="translate(489,0)"><line x1="0" y1="100" x2="150" y2="100" stroke="#7a6f55" stroke-width="1.5"/><path d="M78,100 C88,62 100,62 112,62 C126,62 134,100 150,100 Z" fill="#d9d2c0" stroke="#7a6f55" stroke-width="1.2"/><rect x="20" y="86" width="44" height="14" fill="#5b7393"/><rect x="30" y="76" width="22" height="10" fill="#5b7393"/><line x1="52" y1="81" x2="80" y2="81" stroke="#5b7393" stroke-width="3"/><rect x="34" y="71" width="5" height="5" fill="#5b7393"/><line x1="150" y1="59" x2="0" y2="63" stroke="#a33" stroke-width="1" stroke-dasharray="4,3"/><text x="75" y="122" text-anchor="middle" font-size="11">HIDE</text><text x="75" y="136" text-anchor="middle" font-size="8.5" fill="#555">completely out of sight</text></g><text x="640" y="12" text-anchor="end" font-size="9" fill="#a33">THREAT →  (dashed: line of sight over the crest)</text></svg></div>

- **Hull-down:** only the turret shows over the crest. The tank can fire, and the enemy has a small target that is the most heavily armoured part of the tank. This is the ideal fighting position.
- **Turret-down:** only the commander's sight looks over. The tank can watch but not fire; it moves up a few metres to shoot.
- **Hide:** completely behind cover, waiting.
- **Exposed:** the whole vehicle is visible.

When a platoon occupies a battle position, each tank reports what it found. On the selection panel and status card the task reads **HULL-DOWN** (hull hidden, sight sees most of the threat area), **TURRET-DOWN** (hull hidden, sight sees less than half of it), **EXPOSED** (hull visible, sight sees the threat area) or **MASKED** (hull visible and the view is blocked). Plenty of EXPOSED or MASKED means the battle position is on poor ground: move it.

**Gun depression** matters on reverse slopes: Western tanks can lower their guns about 9–10° below horizontal, Soviet-lineage designs only 4–6°. A position that works for a BULWARK may not work for a KURGAN.

# How the simulation works

You don't need this chapter to play, but it explains *why* things happen. Every number below lives in the data files with its source and a confidence level, and the **DATABASE** screen shows them all.

## Seeing: detection and identification

Detection is **probabilistic over time**, not a simple "in sight / not in sight":

1. **Line of sight** is traced over the ground, including micro-relief, trees, buildings and smoke. Five heights on the target (tracks to sight head) give the *visible fraction*: a hull-down tank shows perhaps 30 %.
2. The sensor's resolution, magnification and range, the light, the atmosphere and the visible fraction give a number of **resolvable cycles** across the target. Using the **Johnson criteria**, about 1 cycle is enough to **detect** something, 4 to **recognise** it as a tank, and 6.4 to **identify** the type.
3. Detection happens after a random search time (the ACQUIRE model). It is much faster when the target **moves** (×1.5) or **fires** (×6, from muzzle flash and dust), and slower when the observer is moving (×0.5) or suppressed.

Each vehicle can have several sensors: the crew's eyes, periscopes, a day sight, image intensifiers or active infrared (older night sights, about 900 m), and **thermal imagers** (see heat, through darkness, dust and most smoke, but not fog). Thermal contrast dips for a while after sunrise and before sunset (**thermal crossover**). The sensors a vehicle has depend on the scenario date, because era is data: a 1987 tank has the sights of 1987.

## Reporting: from crew to map

A crew's detection becomes a **spot report** after 3–12 s (Western) or 5–20 s (Red), with some position error. Reports from several vehicles are fused into contacts. A contact nobody refreshes for a minute fades and shows `LKP` with its age; after three minutes it disappears. Your own vehicles are always shown exactly. (In this version there is no misidentification or friendly fire.)

## Shooting: the engagement sequence

A tank's gunnery is a timeline: traverse the turret, lay the gun, lase the range, let the fire-control computer solve, fire, observe, reload. Times depend on the vehicle's equipment and the crew. Each round's error is the combination of ammunition dispersion, ranging error, residual ballistic error, stabilisation, lead error against moving targets, crosswind and cant. It is sampled and compared with the target's *actual visible silhouette*, so a hull-down target really is harder to hit. Follow-up rounds correct for the first one's error.

The crew chooses its own target from its tracks, according to weapons control ({{ch:Posture}}), and the platoon spreads targets so fire is distributed.

## Penetration and armour

- **Kinetic rounds** (APFSDS "sabot", APDS) punch through armour with a dense, fast rod. Penetration falls with range as the rod slows down; for long rods it follows the Lanz–Odermatt formula.
- **Chemical rounds** (HEAT shells and guided missiles) use a shaped-charge jet. Penetration doesn't fall with range, but composite armour is much more effective against them (the vehicle's CE factor).
- **Explosive reactive armour (ERA)** tiles on the front and sides explode outwards when hit. They remove most of a shaped charge's penetration, and the heavier modern types also degrade sabot rounds. Each tile works once.
- Armour is per face (turret front, hull front, sides, rear, roof) with the impact angle taken into account. **The sides and rear are far weaker than the front.**

## Damage

A penetrating hit can cause a **mobility kill** (M-KILL: can't move), a **firepower kill** (F-KILL: can't shoot), crew casualties, damaged sights, or a **catastrophic kill** (DESTROYED, often an ammunition fire). Non-penetrating hits can still damage sights and suppress the crew. A side is **combat ineffective**, and loses, when 70 % of its vehicles are out of action.

## Movement

Each vehicle's engine power, transmission, weight, track and soil give its speed on every slope and surface. Soft ground, steep side slopes, water deeper than the fording depth and buildings slow or stop vehicles. Rain lowers traction (**GOING SOFT** in the top bar), so vehicles are slower than their plans assume. Fuel is consumed and can run out.

## Time and determinism

The simulation runs at 20 steps per second. The same scenario, seed and orders always give exactly the same battle, which is how replays work.

# The database

The **DATABASE** (main menu, screen menu, or **DATA**/<kbd>I</kbd> on a selected vehicle) shows everything the simulation loads, in five tabs: **VEHICLES**, **AMMUNITION**, **TERRAIN**, **DOCTRINE**, **COMBAT MODELS**.

::fig database-ammo | crop=0,100,1600,710 | pins=1@200,121;2@1527,263;3@1527,397;4@600,530;5@700,121 | The ammunition tab.

| | |
|---|---|
| <span class="pinref">1</span> | Tabs (<kbd>←</kbd>/<kbd>→</kbd> or <kbd>1</kbd>–<kbd>5</kbd>) |
| <span class="pinref">2</span> | Every value has a **confidence**: **HIGH**, **MED**, **LOW** or **EST** (estimate). Hover a value to see its source and notes at the bottom |
| <span class="pinref">3</span> | **CALC** values are computed with the simulation's own formulas: velocity, flight time, drop and penetration by range |
| <span class="pinref">4</span> | "Defeats frontal armour of": the range out to which this round penetrates each vehicle's turret and hull front |
| <span class="pinref">5</span> | **IN SERVICE ALL** or a year: filter by what exists at a date |

Use it to answer practical questions: *Can my 105 mm sabot kill a KURGAN-B from the front? At what range?* (The defeat table ignores ERA and doesn't stop at the round's maximum range, so treat the longest ranges with care.) The **DOCTRINE** tab shows each side's formations, spacings, report delays and engagement ranges; **COMBAT MODELS** explains detection, penetration, damage, smoke, weather, artillery and suppression.

::fig database-doctrine | crop=396,150,1164,380 | w=90 | The doctrine tab for GENERIC OPFOR, with its formation diagrams.

# After-action review and replays

After a battle (or at any time, <kbd>F8</kbd>), the **after-action review** replays the battle with ground truth: every vehicle of both sides where it really was, every shot as a line from shooter to target, and per-side statistics: rounds fired, hits, kills, losses, time of first contact report and hits by range band.

| Control | Effect |
|---|---|
| <kbd>Space</kbd> | Play / pause |
| <kbd>←</kbd> / <kbd>→</kbd> | Jump 10 seconds |
| Drag the timeline | Scrub; tick marks show events and code words |
| <kbd>Ctrl</kbd>+<kbd>R</kbd> | Save a replay to `replays/replay-<scenario>.ron` next to `settings.cfg` (the radio line shows the full path) |
| <kbd>F8</kbd> | Back to the game |

Questions to ask in the AAR: *Who saw whom first, and from where? Which of my positions never fired? Where did my losses come from: front, flank or artillery?* Most lessons are about the first minute of contact.

A replay file holds the scenario, the seed and your orders. `tanks-headless replay --file replay-….ron` re-runs it and checks that it reproduces exactly.

# The editors

## Scenario editor

**SCENARIO EDITOR** on the main menu opens a scenario on its map. You can place, drag, rotate and remove units, make new platoons, choose vehicle types (only those in service at the date), place objectives, and set the date, time, weather, fire support and victory conditions.

The panel on the left has four pages (<kbd>1</kbd>–<kbd>4</kbd> or click the tab). Each row is a value: click it or its **+** for the next value, **-** for the previous one. From the keyboard, <kbd>PgUp</kbd> / <kbd>PgDn</kbd> move the highlight over the rows and <kbd>-</kbd> / <kbd>=</kbd> step the highlighted one.

| Page | Rows |
|---|---|
| **1 UNITS** | Side, doctrine pack, tech year (pretend the side lives in another year), vehicle type, platoon |
| **2 TIME / WX** | Year, month, day, start time (30 min steps), weather preset (clear, haze, overcast, rain, fog, dust), wind direction and speed (drifts smoke), latitude, moon. Below: visibility, sun elevation and light at the start |
| **3 FIRES** | The side's batteries: <kbd>B</kbd> places one at the cursor, LMB selects and drags it, <kbd>Del</kbd> removes it; rows pick the battery, its type and its HE and smoke stock. The selected battery shows its minimum and maximum range |
| **4 VICTORY** | On / off, time limit, the objective the attacker must seize (alone on it for 2 minutes), the attacking side, and the share of its vehicles at which a side loses |

::fig g-scened-wx | crop=0,30,360,340 | w=45 | The TIME / WX page: fog at dawn, with the visibility and light it gives.

::fig g-scened-fires | crop=0,30,1240,860 | w=80 | The FIRES page: the selected battery and its minimum range ring.

Anything that would stop the scenario from starting (a side with no units, a vehicle or battery not in service at the side's year, a victory objective that no longer exists) is listed in red in the panel.

::fig g-scened | crop=0,0,1600,900 | w=90 | The scenario editor on HOLD THE VILLAGE.

| Key | Action |
|---|---|
| <kbd>P</kbd> | Place a unit at the cursor |
| LMB / drag | Select / move a unit |
| <kbd>R</kbd> | Rotate 45° |
| <kbd>Del</kbd> | Remove |
| <kbd>Tab</kbd> | Switch side |
| <kbd>V</kbd> | Next vehicle type (in service at the date) |
| <kbd>N</kbd> | New platoon |
| <kbd>O</kbd> | Objective at the cursor; <kbd>[</kbd> <kbd>]</kbd> radius |
| <kbd>Y</kbd> / <kbd>U</kbd> | Year −/+ |
| <kbd>T</kbd> | Start time +30 min |
| <kbd>B</kbd> | Battery of the side at the cursor |
| <kbd>1</kbd>–<kbd>4</kbd> | Panel page |
| <kbd>PgUp</kbd> / <kbd>PgDn</kbd>, <kbd>-</kbd> / <kbd>=</kbd> | Highlight a row, step its value |
| <kbd>Ctrl</kbd>+<kbd>S</kbd> | Save as `data/scenarios/<id>-custom.ron` |
| <kbd>F9</kbd> | Play test, and back |

Enemy AI plans (the computer side's tasks), side and objective names and the briefing text are edited in the scenario file itself for now. Phase lines, TRPs and contingencies are not part of a scenario: you draw them in the game.

## Terrain editor

**TERRAIN EDITOR** edits a map's authoring package. The map re-bakes in the background after each change.

::fig g-terrained | crop=0,0,1600,900 | w=90 | The terrain editor on the demo map.

| Key | Tool |
|---|---|
| <kbd>1</kbd>–<kbd>4</kbd> | Raise, lower, smooth, flatten (LMB paints; <kbd>[</kbd> <kbd>]</kbd> radius, <kbd>-</kbd> <kbd>=</kbd> rate) |
| <kbd>5</kbd> / <kbd>6</kbd> / <kbd>7</kbd> | Draw river / road / forest: LMB points, <kbd>Enter</kbd> finish, <kbd>Backspace</kbd> undo point |
| <kbd>8</kbd> | Building: LMB centre, LMB end of the long axis |
| <kbd>Del</kbd> | Delete the feature under the cursor |
| <kbd>Ctrl</kbd>+<kbd>Z</kbd> / <kbd>Ctrl</kbd>+<kbd>Y</kbd> | Undo / redo |
| <kbd>B</kbd> | Bake now |
| <kbd>Ctrl</kbd>+<kbd>S</kbd> | Save |

Scenarios on the map use the saved version the next time they start. Maps are stored as GeoTIFF and GeoJSON, so they can also be edited in QGIS.

# For tinkerers

- **Data files.** Vehicles, rounds, doctrine, weather, smoke, artillery, terrain and scenarios are plain-text RON files under `data/`. Every number has a `source` and a `confidence`. Vehicle names are fictional, but each sheet says which real design it mirrors.
- **Doctrine packs** (`data/doctrine/<pack>/doctrine.ron`) set formations, spacing, bound length, report and order delays, engagement range, drills, code words and march speeds. A scenario assigns a pack to each side.
- **`tanks-headless`** generates and bakes maps (`gen`, `bake`), renders map images (`render`), runs scenarios with both sides computer-controlled (`run --scenario … --seed n --summary` prints one CSV line with the winner and losses), runs gunnery duels (`duel`), and verifies replays (`replay`).
