<div class="part"><div class="pn">PART I</div><div class="pt">Basics</div>
<p>What the game is, how to start it, how to read the screen and the map, how to give the basic orders, and a complete first battle step by step.</p></div>

# What kind of game is this?

## You are the company commander

In *Tanks* you play a **company commander**: an officer in charge of two or three **platoons** of four tanks (or three fighting vehicles) each, sometimes with artillery on call. A platoon is the smallest group you normally give orders to. Inside each platoon a **platoon leader** (computer controlled) decides which tank goes where, and each tank's **crew** decides what to look at and what to shoot.

You watch a **tactical map**: contour lines, forests, roads, villages, your own vehicles as blue symbols, and enemy vehicles as red or yellow symbols *where your crews last reported them*. There is no 3D view. That is deliberate. Real commanders at this level fight from a map and a radio.

## Command, don't control

Most real-time strategy games let you steer every unit like a puppet. *Tanks* doesn't. You give a platoon an **intent**, and the platoon works out the details:

- "Move to that hill" → the platoon picks a route across the ground, keeps a formation and adjusts its spacing to the terrain.
- "Occupy a battle position here, facing north-east" → the platoon leader searches the area for spots where each tank's hull is hidden behind a fold in the ground but its gun can see the threat, and spreads the four tanks out.
- When a crew sees an enemy, it reports it, the platoon halts and turns towards the threat, picks targets so two tanks don't shoot the same enemy, and resumes the move once things go quiet. This is a **battle drill**: a practised reaction that needs no order from you.

You can still intervene ("halt", "engage that target", "break contact"), but every order costs time on the radio. The better your initial plan, the less you need to.

## You only see what is reported

Everything enemy on your map comes from a **spot report** by one of your crews. A crew has to *detect* something first, which depends on range, light, weather, smoke, the enemy's cover and whether it is moving or firing. The report then takes a few seconds to come over the radio, and its position can be a little off. Contacts that nobody sees any more fade, turn into a **last known position** and eventually disappear. Ground truth, meaning where everything really was, is only shown afterwards, in the **after-action review**.

## Terrain is the main character

A one-metre rise in the ground can hide a tank's hull. A forest edge can block every sight line. Whoever sees first usually shoots first, and whoever shoots first usually wins. Most of this guide comes back to one idea: **use the ground**.

## Real time, with pause

The clock runs in real time (or up to eight times faster) and you can pause at any moment, give orders and resume. Each scenario starts in a **planning phase** with the clock stopped. Orders given then take effect instantly at H-hour (the start time), without radio delay.

<div class="tip" markdown="1">
<span class="label">If you have played other strategy games</span>
Forget about micromanaging individual tanks. Spend the planning phase choosing good positions, put your platoons in them, and let the crews fight. Most early losses come from moving across open ground in view of the enemy, not from slow clicking.
</div>

# Installing and starting

## What is in the package

A release package (`tanks-<version>-<platform>.tar.gz` or `.zip`) holds:

| File | What it is |
|---|---|
| `tanks` | The game, using OpenGL 3.3 (any graphics card from about 2010 on) |
| `tanks-gl21` | The same game for older graphics (OpenGL 2.1, roughly 2006–2012 hardware). Use it if `tanks` fails to open a window |
| `tanks-headless` | A command-line tool for map generation, batch runs and replay checks. You don't need it to play |
| `data/` | Vehicles, ammunition, doctrine, scenarios and the two pre-generated maps (`demo`, `uplands`) |

Unpack it anywhere and run `tanks` (`tanks.exe` on Windows). The game finds `data/` next to the executable. The packages are unsigned, so Windows SmartScreen and macOS Gatekeeper will warn the first time.

The window is 1600 × 900. <kbd>F11</kbd> toggles borderless fullscreen at any time.

## The title screen and main menu

The game opens on a title screen: press any key. The **main menu** has six entries:

::fig main | crop=100,140,460,470 | w=55 | The main menu. Every menu works with the mouse or with <kbd>↑</kbd>/<kbd>↓</kbd>, <kbd>Enter</kbd> and <kbd>Esc</kbd>.

| Entry | Opens |
|---|---|
| **PLAY SCENARIO** | The scenario list and briefing (next section) |
| **SCENARIO EDITOR** | Place units, objectives and batteries, set the date, time, weather and victory conditions, then play-test ({{ch:The editors}}) |
| **TERRAIN EDITOR** | Raise and lower ground, draw rivers, roads, forests and buildings ({{ch:The editors}}) |
| **DATABASE** | Every vehicle, round, terrain type, doctrine and combat model the simulation uses, with sources ({{ch:The database}}) |
| **OPTIONS** | Display and control settings (below) |
| **QUIT** | Leaves the game |

When you leave a running game for the main menu, it is suspended and a **RESUME** entry appears at the top. Starting another scenario discards the suspended one.

## Options

::fig options | crop=500,120,600,662 | w=62 | The options screen. Changes are saved at once.

| Option | Values | Notes |
|---|---|---|
| **DISPLAY** | WINDOW / FULLSCREEN | Borderless fullscreen. Also <kbd>F11</kbd> |
| **FRAME RATE** | 30 / 60 / 120 FPS / VSYNC | Cap on screen refresh. The simulation itself always runs at 20 steps per second |
| **UI FONT** | IBM PLEX MONO / BUILT-IN | The built-in font is a fallback for very old hardware |
| **PAN SPEED** | SLOW / NORMAL / FAST | Arrow-key panning speed |
| **EDGE PAN** | ON / OFF | Scroll the map when the mouse touches a screen edge |
| **CONTOURS**, **GRID** | ON / OFF | Whether these map layers are shown when a map opens |
| **PAUSE ON MENU** | ON (default) / OFF | The clock stops while a context menu is open or you are editing a plan. Recommended |
| **PAUSE WHEN SET** | OFF (default) / ON | The clock stops when every platoon waiting for a code word reports it is in position ({{ch:Code words}}) |
| **AUTO-PAUSE** | OFF / NEW CONTACT / CONTACT, LOSS / CONTACT, LOSS, FIRE | Stop the clock on a new enemy contact, a vehicle of yours destroyed, or incoming artillery |

Options are stored in a small text file: `~/.config/tanks/settings.cfg` on Linux, the equivalent configuration folder elsewhere, or the path in the `TANKS_CONFIG` environment variable. The path is shown at the bottom of the options screen.

<div class="tip" markdown="1">
For your first games, set **AUTO-PAUSE** to **CONTACT, LOSS**. The game then stops whenever something important happens, so you can look at the situation and react calmly.
</div>

## Choosing a scenario

**PLAY SCENARIO** shows the scenario list on the left and the selected scenario's details on the right.

::fig play | crop=0,0,1600,900 | pins=1@260,188;2@560,219;3@560,241;4@560,275;5@560,307;6@560,329;7@560,402;8@1390,548;9@1420,839 | The scenario select screen.

| | |
|---|---|
| <span class="pinref">1</span> | The scenario list. The year is on the right. Use <kbd>↑</kbd>/<kbd>↓</kbd> or click |
| <span class="pinref">2</span> | **DATE** and **START** time. These decide the light (sun, moon) and which vehicles and rounds are in service |
| <span class="pinref">3</span> | **MAP**: map name, the recipe it was generated from, its seed and size |
| <span class="pinref">4</span> | **SIDES**: each side's strength, doctrine pack, and who controls it. You always command the first side. **COMPUTER (PLAN)** means the enemy follows a scripted plan with its own commander AI |
| <span class="pinref">5</span> | **OBJECTIVES**: named areas that matter for victory, usually a village |
| <span class="pinref">6</span> | **VICTORY**: time limit, the loss level at which a side is beaten (usually 70 %), and what has to be held |
| <span class="pinref">7</span> | **BRIEFING**: situation, mission and notes. *Read the notes*: they often contain the key to the scenario |
| <span class="pinref">8</span> | A preview of the map |
| <span class="pinref">9</span> | **START** (<kbd>Enter</kbd>) loads the map and opens the game |

The first time a map is used, the game may generate and "bake" it (precompute line-of-sight and mobility data) on the loading screen. That takes a few seconds. After that it loads quickly.

<div class="tip" markdown="1">
<span class="label">Which scenario first?</span>
Start with **HOLD THE VILLAGE** (1990): two tank platoons defend a crossroads village against a Red tank company. It is the easiest scenario, and {{ch:Your first battle}} walks through it. **M1 - MOVEMENT** has no fighting at all and is good for practising movement orders. The six scenarios from 1986, 1987 and 2005 are the real campaign-grade content. See {{ch:Scenario hints}} and {{ch:Scenarios}}.
</div>

## Help, the screen menu and quitting

- <kbd>F7</kbd> (or **HELP** in the top bar) shows the keyboard and mouse reference for the current screen.
- <kbd>Esc</kbd> backs out of whatever you are doing: a pending click, a tool, a selection. When there is nothing left to cancel, it opens the **screen menu**: resume, after-action review, restart, options, help, main menu, quit. <kbd>F10</kbd> opens the menu directly.

::fig g-help | crop=448,266,704,370 | w=85 | The in-game key reference (<kbd>F7</kbd>). {{ch:Keyboard and mouse reference}} has the same list with explanations.

# The game screen

The game screen is busy, but every part has one job. This chapter names each part; later chapters explain how to use them.

::fig tour | pins=1@40,15;2@300,15;3@560,15;4@870,15;5@1335,15;6@125,78;7@125,152;8@1440,62;9@1440,262;10@745,300;11@880,467;12@230,672;13@95,780;14@530,780;15@1240,790;16@150,885 | The game screen during HOLD THE VILLAGE, 3 minutes 51 seconds after H-hour. 1st Platoon is following a checkpoint plan and waits at checkpoint 2 for the code word HAMMER; 2nd Platoon is set in a battle position east of the village.

| | Area | What it shows and does |
|---|---|---|
| <span class="pinref">1</span> | Clock and state | Scenario time (hh:mm:ss) and **PLANNING**, **RUNNING X*n*** or **PAUSED** |
| <span class="pinref">2</span> | Time controls | **EXECUTE** (ends the planning phase) or **PAUSE/RESUME**; **X1 X2 X4 X8** time compression; **AAR** after-action review |
| <span class="pinref">3</span> | Side and scenario | Your side's name and the scenario |
| <span class="pinref">4</span> | Light and weather | **DAY / DUSK / NIGHT**, the weather preset (**CLEAR**, **FOG** …), visibility, and **GOING SOFT** when rain makes the ground slow |
| <span class="pinref">5</span> | Map layers and help | **RELIEF / SLOPE / GO/NO-GO / COVER** base maps, **CONTOURS**, **GRID**, **HELP**, **MENU** |
| <span class="pinref">6</span> | Order of battle | Your platoons with strength (operational/total) and their current task. Click to select; click a selected platoon again to centre the map on it |
| <span class="pinref">7</span> | Code words / fire support | Code words your plans wait for, and who is ready ({{ch:Code words}}). In scenarios with artillery, a **FIRE SUPPORT** box shows each battery's rounds and state |
| <span class="pinref">8</span> | Selection panel | The selected platoon: vehicle type, task, formation, technique, speed, strength, ammunition, fuel, then one row per vehicle. For a single vehicle: its status card ({{ch:Selecting units}}). **DATA** opens its database sheet |
| <span class="pinref">9</span> | Plan step list | When the platoon has a plan: one row per checkpoint with its ETA and settings |
| <span class="pinref">10</span> | Tactical map | Terrain, your units, contacts, routes, plans and control measures |
| <span class="pinref">11</span> | Ghost order | Where a right click would send the selection: distance, estimated time, and what the click would do (**REPLACES PLAN**, **ADD CP 3** …) |
| <span class="pinref">12</span> | Hint line | What each mouse button does right now. Read it whenever you are unsure |
| <span class="pinref">13</span> | Minimap | The whole map with the visible area outlined. Click or drag to move the view |
| <span class="pinref">14</span> | Radio net | Every order you send and every report you receive, time-stamped. This is your main source of information |
| <span class="pinref">15</span> | Command card | Every order as a button with its hotkey, in three rows: **ORDERS**, **POSTURE**, **FIRES/STAFF** (Chapters {{chn:Basic orders}}–{{chn:Fires and staff tools}}) |
| <span class="pinref">16</span> | Status bar | Under the cursor: grid reference, elevation, slope, land cover (and water depth). Right: the base layer and map scale |

<div class="note" markdown="1">
**Hover over anything.** Every button has a tooltip that names its hotkey and says *who* the order goes to and *when* (for example `ORDERS · 1 NOW: MOVE TO A POINT …`).
</div>

# Reading the map

## Terrain

::fig g-uplands-zoom | crop=0,30,1270,660 | pins=1@497,105;2@380,280;3@260,450;4@580,620;5@818,200;6@683,38 | Wooded uplands at 2.4 m per pixel.

| | Feature | How it is drawn |
|---|---|---|
| <span class="pinref">1</span> | Contour lines | Lines of equal height: thin every 10 m, thicker and labelled every 50 m (here "250"). Lines close together mean steep ground. At high zoom, 5 m lines appear |
| <span class="pinref">2</span> | Relief | Shading as if lit from the north-west, plus a light tint by height. It helps you see ridges and valleys at a glance |
| <span class="pinref">3</span> | Forest | Dark green with a dot pattern. Forest blocks sight after a few dozen metres and slows vehicles |
| <span class="pinref">4</span> | Village | Grey area with building outlines. Buildings block sight and movement |
| <span class="pinref">5</span> | River | Blue line. Shallow water can be forded; deep water needs a bridge (a road over a river is a bridge) |
| <span class="pinref">6</span> | Grid | 1 km squares, labelled on the left and top edges with the kilometre number ("04" = 4 km) |

Roads are thin double lines. Tracks and roads are faster to move on, but they are also where the enemy expects you.

## Base map layers

<kbd>F1</kbd>–<kbd>F4</kbd> (or the top-bar buttons) switch the base map. <kbd>F5</kbd> toggles contours and <kbd>F6</kbd> the grid.

::fig g-layer-slope | crop=0,30,1270,660 | w=48 | **SLOPE** (<kbd>F2</kbd>): darker means steeper.
::fig g-layer-traffic | crop=0,30,1270,660 | w=48 | **GO/NO-GO** (<kbd>F3</kbd>): how fast tracked vehicles can cross the ground. Fast going is light, slow is darker, impassable is marked.

::fig g-layer-cover | crop=0,30,1270,660 | w=48 | **COVER** (<kbd>F4</kbd>): land cover (crops, grass, forest, marsh, village …).
::fig g-night | crop=0,30,1270,660 | w=48 | **RELIEF** (<kbd>F1</kbd>), the default. The map is drawn the same by day and by night; the top bar tells you the light.

## Grid references, distances and bearings

Positions are given as **grid references**, measured from the south-west corner of the map:

- **6 digits**, as in `GRID 045 059`: 4.5 km east, 5.9 km north, to the nearest 100 m. The radio uses this form.
- **8 digits**, as in `GRID 0490 0470`: 4.90 km east, 4.70 km north, to the nearest 10 m. The status bar uses this form.

Always read **east first, then north** ("in the door and up the stairs").

Directions are **bearings in mils**, the military unit for angles. A full circle is **6400 mils**: north is `0000`, east `1600`, south `3200`, west `4800`. North-east is `0800`. One mil is almost exactly one metre of width at one kilometre of range, which is why gunners like it. Speeds are shown in km/h and distances in metres or kilometres.

## Symbols

The symbols follow the NATO standard (APP-6 / MIL-STD-2525) in simplified form. Colour is never the only clue: the frame shape also tells you who is who.

| Symbol | Meaning |
|---|---|
| Blue **rectangle** with an oval inside | Your vehicle (friendly armour). The label is its callsign, such as `1-2` (platoon 1, vehicle 2) |
| Yellow corner brackets | Selected |
| Rectangle with three dots above it | A whole platoon, shown instead of the vehicles when you zoom out far |
| Yellow **four-lobed frame**, label `UNK` | Unknown contact: something was detected, but nobody can tell what it is |
| Red **diamond** with an oval, label `TANK` | Recognised hostile vehicle (it is a tank-like vehicle, type not yet known) |
| Red diamond, label such as `KURGAN-B` | Identified hostile: the type is known |
| Faded symbol with `LKP 75S` | Last known position: nobody has seen it for 75 seconds. Contacts expire after about three minutes without a report |
| Cross over a symbol | Destroyed. A single slash means damaged (mobility or firepower kill) |
| Grey discs | Smoke clouds. They drift with the wind and thin out |

::fig g-contacts-zoom | crop=560,180,480,260 | w=70 | Contacts after a Red column was engaged: destroyed tanks (crossed diamonds) labelled `KURGAN-B` and `TANK`, a yellow unknown contact (`UNK`), and grey smoke from the crews' self-screening grenades.

Your own orders and control measures are drawn too: routes as lines with numbered checkpoints and an **X** at the destination, battle positions as circles with an arrow towards the threat, phase lines as long lines labelled **PL RED**, **PL BLUE** …. **Dashed** means "ordered, not yet acknowledged" (or a draft); **solid** means "being carried out".

## Moving the view

| Action | Mouse | Keyboard |
|---|---|---|
| Zoom | Wheel (zooms at the cursor) | |
| Pan | Drag with the middle button; click or drag on the minimap | Arrow keys |
| Centre on a platoon | Click it in the order of battle when it is already selected | |

The status bar shows the scale (metres per pixel). Below about 7 m per pixel you see individual vehicles; zoomed further out you see one symbol per platoon.

# Time, planning and the radio

## The planning phase

Every scenario starts with the clock stopped at H-hour and **PLANNING** in the top bar. During planning:

- every order and control measure you give takes effect **immediately**, without radio delay,
- you can study the map, draw phase lines, set up battle positions and build plans,
- the enemy doesn't move.

Press **EXECUTE** (<kbd>Enter</kbd>) to start the clock. You can't go back to planning afterwards.

## Pause and time compression

| Control | Key | Effect |
|---|---|---|
| **PAUSE / RESUME** | <kbd>Space</kbd> | Stop or restart the clock. You can give orders while paused |
| **X1 X2 X4 X8** | <kbd>1</kbd>–<kbd>4</kbd> | Time compression. Choosing a speed also resumes |

Time compression runs more simulation steps per second; it never makes the simulation coarser, so the result is the same at any speed. Use X4 or X8 for long approach marches and X1 when the shooting starts.

## Orders travel over the radio

Once the clock runs, an order is not received instantly. It has to be spoken over the radio net and acknowledged. Your platoons take **5 to 20 seconds** (Western doctrine) to reply `ROGER, OUT` and start acting. Red's tighter, more centralised doctrine takes 10 to 30 seconds. Until the acknowledgement arrives, the order's graphic stays dashed.

That delay is why **pre-planned orders** are so valuable: a plan or contingency crosses the radio once, before the fight, and later steps happen instantly.

# Selecting units

| To select | Do this |
|---|---|
| A platoon | LMB on any of its vehicles, or on its row in the order of battle |
| The next platoon | <kbd>Tab</kbd> |
| A single vehicle | <kbd>Ctrl</kbd>+LMB on it, or LMB on its row in the selection panel |
| Several platoons | <kbd>Shift</kbd>+LMB adds or removes a platoon; or drag a box on empty ground (<kbd>Shift</kbd> adds to the selection) |
| A saved group | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>1</kbd>–<kbd>9</kbd> stores the selection; <kbd>Shift</kbd>+<kbd>1</kbd>–<kbd>9</kbd> recalls it |
| Nothing | <kbd>Esc</kbd> (when no tool is active), or LMB on empty ground |

::fig x-box | crop=560,480,480,200 | w=60 | Box selection: drag on empty ground. Every platoon with a vehicle inside the box is selected.

With several platoons selected, the command card reads `PLATOON 1 + 2`. Orders go to every selected platoon; move orders keep each platoon's offset from the first one, so they don't pile onto one point. Plans are built for one platoon at a time.

## The selection panel

::fig tour | crop=1274,36,322,290 | w=48 | A platoon selected: task (**OCCUPYING BATTLE POSITION**), formation, technique, speed, strength (`UP 4/4`), main-gun rounds, lowest fuel, then one row per vehicle, then the plan's checkpoints.
::fig g-status-card | crop=1274,36,322,300 | w=48 | One vehicle selected: its status card, which shows what the crew reports.

The vehicle **status card** shows:

| Row | Meaning |
|---|---|
| Condition, crew | **OPERATIONAL**, **M-KILL** (can't move), **F-KILL** (can't shoot) or **DESTROYED**; crew members fit (`CREW 4/4`) |
| **TASK** | What it is doing. In a battle position: **HULL-DOWN**, **TURRET-DOWN**, **EXPOSED** or **MASKED** ({{ch:Terrain analysis}}) |
| **GRID**, **HDG** | Position and hull heading in mils |
| **SPEED** | Current speed, with the road speed limit in brackets |
| **FUEL** | Percentage and litres |
| **MAIN GUN** | Rounds left of total, and the gun's state: **READY**, **LAYING**, **LASING**, **RELOADING** |
| **ROUNDS** | Rounds by type: **APFSDS** (armour-piercing "sabot"), **HEAT**, **SMOKE** … |
| **AMMO SEL** | **AUTO** or a type you told it to load |
| **TURRET**, **WEAPONS** | Where the gun points; weapons control ({{ch:Posture}}) |
| **SMOKE** | Smoke-grenade salvos left |
| **TARGET** | What the crew is engaging: type, range, direction |
| **SIGHTS** | Sights in service: **DAY**, **THERMAL**, **NIGHT** (image intensifier or infrared), or damaged |

**DATA** (<kbd>I</kbd>) opens the vehicle's sheet in the database with the game paused underneath.

# Basic orders: moving and positions

## The command card

::fig tour | crop=884,692,712,180 | pins=1@934,795;2@1181,771;3@1133,771;4@1184,806 | The command card for 1st Platoon.

| | |
|---|---|
| <span class="pinref">1</span> | Row label: who the row orders and when. `1 NOW` means "platoon 1, immediately". With a checkpoint selected, the posture row reads `1 LEG CP 2`: picks then change that leg of the plan instead |
| <span class="pinref">2</span> | A button (here MOVE SPEED). Greyed buttons are unavailable for the current selection |
| <span class="pinref">3</span> | Its hotkey |
| <span class="pinref">4</span> | Its current value (here **15 KM/H**; elsewhere formation **WEDGE**, 16 smoke rounds …) |

Hover over a button for its tooltip, which repeats the scope:

::fig x-tooltip | crop=996,752,552,22 | The tooltip of BATTLE POS.

## Move: right click

**Right-click the map** to move the selection there. A dashed **ghost line** follows the cursor before you click and shows the distance and estimated travel time. Its label also warns you when the click will **REPLACE** an existing plan.

The platoon plans its own route: it prefers fast ground and roads, avoids water, buildings and impassable slopes, and keeps its formation. **MOVE** (<kbd>M</kbd>) then LMB does the same as a right click.

::fig g-route-moving | crop=0,30,1270,660 | pins=1@675,400;2@700,650;3@849,150;4@880,467 | A move with two checkpoints, 45 seconds after H-hour.

| | |
|---|---|
| <span class="pinref">1</span><span class="pinref">2</span> | Checkpoints (CPs): points the platoon must pass, in order |
| <span class="pinref">3</span> | The destination (**X**) |
| <span class="pinref">4</span> | The ghost order for the *next* right click |

**Checkpoints:** hold <kbd>Shift</kbd> and right-click several times; each click adds a checkpoint. Release <kbd>Shift</kbd> and the whole route goes out as **one** radio order. {{ch:Routes, drafts}} covers this in detail.

::fig x-shift-append | crop=420,140,800,520 | w=80 | Holding <kbd>Shift</kbd>: two checkpoints placed, the ghost says **ADD CP 3**, and the draft strip at the bottom right counts the changes. Releasing <kbd>Shift</kbd> sends the route.

## Halt and resume

**HALT** (<kbd>H</kbd>) stops the selection where it is but *remembers* the order it was carrying out. The task reads **HALTED, ROUTE HELD**, and the button turns into **RESUME**, which continues from where the unit stopped. Any new move or battle-position order discards the held one.

::fig x-halted | crop=0,30,1600,850 | pins=1@125,66;2@1070,735 | 1st Platoon halted on its way north-east. The order of battle reads **HALTED, ROUTE HELD** <span class="pinref">1</span>; the card offers **RESUME** <span class="pinref">2</span>. Note the radio: HALT sent at 06:00:25, acknowledged 14 seconds later.

## Battle position

A **battle position** (BP) is an area where a platoon sets up to fight, facing the direction the enemy is expected from. It is the most important order in the game.

1. Select a platoon and press **BATTLE POS** (<kbd>B</kbd>).
2. LMB the centre of the area.
3. LMB again in the direction of the threat. An arrow follows the mouse.

::fig x-bp-tool | crop=560,30,700,300 | w=75 | Placing a battle position: centre clicked, the arrow shows the threat direction. <kbd>[</kbd> and <kbd>]</kbd> change the radius (100–800 m, default 250 m).

The platoon leader then searches the circle for the best spot for each tank: hull hidden from the threat but gun and sights able to see it (**hull-down**), spread out, with the gun able to depress far enough. When all tanks are in place the radio says, for example:

`2: SET IN BP. 0 HULL-DOWN, 4 TURRET-DOWN, 0 EXPOSED. ORIENTED 0800 MILS.`

How many positions are truly hull-down depends on the ground. Flat crop fields give few, ridges and folds give many. If the report says mostly **EXPOSED**, look for better ground ({{ch:Terrain analysis}}).

## Engage, reverse, break contact, smoke

| Button | Key | What it does |
|---|---|---|
| **ENGAGE** | <kbd>A</kbd>, or <kbd>Ctrl</kbd>+RMB on a contact | The selected *vehicle* engages that contact. Platoons distribute fire themselves, so you rarely need this |
| **REVERSE** | <kbd>D</kbd> | Back up 100 m with the hull and gun still facing the same way. This is how a tank leaves a firing position without showing its thin rear armour |
| **BREAK CONT** | <kbd>X</kbd> | The *break contact* battle drill: fire smoke, then withdraw about 800 m away from the nearest known threat. It replaces the platoon's plan |
| **SMOKE** | <kbd>Z</kbd> | Fire a smoke-grenade salvo towards the nearest known threat (blocks sight for about a minute). Crews also do this on their own when they come under fire |

<div class="warn" markdown="1">
**Smoke grenades** of the 1980s block visible light only. Thermal sights see through them. From 1998 on, multispectral grenades also block thermal sights. Check the database (*COMBAT MODELS → SMOKE*).
</div>

# Posture: how your platoons move and fight

The second row of the command card sets *how* a platoon behaves. Each button opens a list at the button; pick with the mouse, the number keys or the arrows and <kbd>Enter</kbd>. The current value is marked with `*`.

::fig h-list-F | crop=900,440,700,440 | w=60 | The FORMATION list. With no checkpoint selected the title reads `FORMATION · 1 · NOW`, so the order goes to platoon 1 immediately.

If a checkpoint of the platoon's plan is selected, the list title reads `LEG TO CP n` and the pick changes only that leg of the plan. An extra **KEEP** entry means "this leg doesn't change the setting".

## Formation (F)

How the four tanks are arranged relative to the leader. Distances are in multiples of the doctrine's spacing (100 m for Western doctrine).

::formations western-combined

| Formation | Use it when | Trade-off |
|---|---|---|
| **COLUMN** | Moving on roads, through woods or narrow gaps; fast and easy to control | Only the lead tank can fire forward; terrible if surprised from the front |
| **STAGGERED COLUMN** | Road marches when contact is possible | A little more firepower to the flanks |
| **WEDGE** | The default for moving in open country when you don't know where the enemy is | Good all-round firepower and control |
| **VEE** | You expect contact to the front and want two tanks forward | Less firepower to the rear |
| **LINE** | Assaulting, or crossing a danger area with maximum firepower to the front | Hard to control; flanks are weak |
| **ECHELON LEFT / RIGHT** | Protecting a flank you know is threatened | The other flank is weak |

## Movement technique (T)

| Technique | What the platoon does | Use it when |
|---|---|---|
| **TRAVELING** | Everyone moves at once, as fast as the formation allows | Contact is unlikely; speed matters |
| **TRAVELING OVERWATCH** | The lead section moves; the trailing section follows further back so it can react if the lead is hit | Contact is possible |
| **BOUNDING OVERWATCH** | One section (two tanks) stops in a good spot and watches while the other moves up to about 600 m ("bounds"), then they swap | Contact is expected. Slow but safe |

The radio reports each bound: `1: SECTION 2 BOUNDING.`

## Move speed (S)

**MAX** or one of the doctrine's ordered speeds (40, 25, 15 km/h). A speed is a ceiling for every vehicle, so a mixed platoon stays together. MAX is 60 % of the slowest member's cross-country speed. Slower movement raises less dust and keeps the formation tidy; fast movement crosses danger areas sooner.

## Weapons control (W)

| Status | Crews open fire on | Use it when |
|---|---|---|
| **FREE** | Anything they *recognise* as a hostile vehicle | The normal setting in combat |
| **TIGHT** | Only targets they have positively *identified* (type known) | Friendly units may be nearby, or you want to save the ambush for real targets |
| **HOLD** | Nothing except targets you assign with **ENGAGE** | Ambushes and hidden observation: firing gives your position away |

## On contact (C)

What the platoon does when a crew reports an enemy:

| Reaction | Behaviour |
|---|---|
| **DRILL (HALT)** | The default battle drill: halt, turn towards the threat, engage, and resume the move when things are quiet for a while |
| **CONTINUE** | Keep moving and fire on the move. Use it when stopping in the open would be worse, for example during an assault or to reach a position |
| **HASTY POSITION** | Look for hull-down ground close by facing the threat, occupy it, and fight from there |
| **BREAK CONTACT** | Smoke and withdraw immediately. This ends the platoon's plan |

## Ammunition (Q) and engine smoke (E)

**AMMO** is **AUTO** by default: crews pick the round for the target (sabot against tanks, HEAT or HE against lighter vehicles). You can hold one type instead, for example HEAT against fighting vehicles. **ENG SMOKE** toggles engine exhaust smoke on vehicles that have it (mostly Red designs), which lays a smoke trail behind moving vehicles.

# Fires and staff tools

The third row holds tools a commander uses besides giving movement orders. Each is covered in detail in Part II.

| Button | Key | Purpose | Chapter |
|---|---|---|---|
| **SMOKE RND** | <kbd>N</kbd> | Tank-gun smoke rounds at a point: blind a distant enemy position | {{chn:Smoke}} |
| **CALL FIRE** | <kbd>Y</kbd> | Artillery fire mission (HE or smoke) on a point. Press <kbd>Y</kbd> again to switch HE/SMOKE | {{chn:Fire support}} |
| **ADJUST** | <kbd>J</kbd> | Correct the last artillery mission onto where it should have landed | {{chn:Fire support}} |
| **TIMING** | <kbd>K</kbd> | Make the *next* order a contingency: **ON CROSSING** (when an enemy crosses the last phase line drawn) or **+5 MIN** | {{chn:Control measures}} |
| **GRAPHICS** | <kbd>G</kbd> | Draw a phase line, objective or target reference point | {{chn:Control measures}} |
| **VIEWSHED** | <kbd>V</kbd> | Shade everything visible from a point | {{chn:Terrain analysis}} |
| **LOS** | <kbd>L</kbd> | Line of sight between two points, with a height profile | {{chn:Terrain analysis}} |

# Reading the radio net

The radio net is your eyes and ears. Reports follow military brevity: who is speaking, then the facts, in a fixed order. Examples from a real game:

| Message | Meaning |
|---|---|
| `TO 1: MOVE VIA CP 1 GRID 043 040, CP 2 GRID 042 045, TO GRID 046 050, TRAVELING.` | Your order to 1st Platoon |
| `1: ROGER, OUT.` | 1st Platoon acknowledges and starts |
| `1-3: CONTACT, TANK, GRID 045 059, DIRECTION 0002, 1.5 KM. 300 M BEYOND PL RED.` | Vehicle 1-3 reports a new contact: a tank-like vehicle at grid 045 059, bearing 0002 mils (just east of north), 1.5 km away, 300 m beyond your phase line RED |
| `2-2: UPDATE, KURGAN-B, GRID 045 058, DIRECTION 0079, 1.4 KM.` | 2-2 has a better look at a known contact: it's a KURGAN-B |
| `1-3: TARGET HIT.` / `1-3: TARGET DESTROYED.` | Result of 1-3's shot |
| `2-4: HIT, NO PENETRATION.` | 2-4 was hit but the armour held |
| `1: SET IN BP. 0 HULL-DOWN, 2 TURRET-DOWN, 2 EXPOSED. ORIENTED 0800 MILS.` | 1st Platoon is in its battle position, and how well hidden |
| `1: CONTACT, DIRECTION 0050, 1.6 KM. ACTION, ENGAGING.` | The platoon is executing the contact drill |
| `1: CP 2. WAITING HAMMER.` | 1st Platoon reached checkpoint 2 and waits for the code word HAMMER |
| `ALL STATIONS, CODE WORD HAMMER.` | You sent the code word |
| `1: UNDER FIRE, MOVING TO CP 4.` | A plan branch fired ({{ch:Checkpoint plans}}) |
| `ARTY 1: FIRE MISSION, GRID 051 047. SHOT IN 95 SECONDS, OVER.` | The battery accepted your call for fire |
| `ARTY 1: SHOT, OVER.` / `SPLASH, OVER.` | Guns fired / rounds land in 5 seconds |
| `2: INDIRECT FIRE, GRID 044 046. BUTTONED UP, HOLDING.` | 2nd Platoon is under artillery fire and reacts ({{ch:Fire support}}) |
| `1-2: BLACK ON FUEL AT GRID …` | Out of fuel |
| `ENDEX. RED COMBAT INEFFECTIVE. BLUE WINS.` | End of exercise: the scenario is over |

Older messages scroll up; the panel keeps the last 300.

# Your first battle: Hold the Village

This chapter plays **HOLD THE VILLAGE** from start to finish. The situation: it is 06:00 on a June morning in 1990. A Red tank company (nine KURGAN-B tanks, modelled on the T-72B) is expected from the north-east. You have two platoons of four BULWARK A1 tanks (modelled on the M1A1) south of a crossroads village. You must hold the village for 45 minutes.

## Step 1: Read the briefing and the map

The briefing's notes say: *occupy battle positions (B) facing NE, draw a phase line, then execute (Enter)*. Before doing anything, look at the map:

- The village sits on a crossroads with open, gently rolling cropland to the north and north-east. A road runs north out of the village. That is the obvious approach for Red.
- There are patches of forest east and north-east. Red can hide in them, but they also block Red's view.
- The enemy will come from the north-east, so your tanks should face that way, using the village and small rises as cover.

## Step 2: Put each platoon in a battle position

1. Click **1 PLT** in the order of battle.
2. Press <kbd>B</kbd>, click a centre point just north-east of the village, then click towards the north-east.
3. Press <kbd>Tab</kbd> for 2nd Platoon, and do the same on the north-west side, so both platoons can fire into the open ground north of the village from different angles.

## Step 3: Draw a phase line

A **phase line** is a line on the map with a name, used to coordinate and to report. Draw one across Red's likely approach, about 1.5 km out:

1. Press <kbd>G</kbd>, then choose **PHASE LINE**.
2. Click three or four points across the approach.
3. Press <kbd>Enter</kbd> (or RMB) to finish. It is named **PL RED**, the next one **PL BLUE**, and so on.

Spot reports will now tell you where contacts are relative to it (`300 M BEYOND PL RED`), and you can make orders wait for "enemy crosses PL RED".

::fig g-planned | crop=0,30,1600,850 | pins=1@560,481;2@750,543;3@1440,482 | The plan at H-hour: two battle positions <span class="pinref">1</span><span class="pinref">2</span> facing north-east (0800 mils) and phase line RED <span class="pinref">3</span>. The graphics are still dashed because the clock hasn't started.

## Step 4: Execute and watch them set

Press <kbd>Enter</kbd> (**EXECUTE**). The platoons move into their positions. Use **X4** or **X8** while nothing happens. After about two minutes the radio reports both platoons **SET IN BP**.

::fig g-set | crop=400,330,560,330 | w=75 | Both platoons set at 06:02. Each vehicle row in the selection panel shows its position quality (here **TURRET-DOWN**).

## Step 5: Wait, and drop to X1 when contact comes

Red approaches in column along the road from the north. Around 06:10 the first spot reports arrive. Switch to **X1** so you can follow the fight.

::fig f660 | crop=0,30,1600,850 | pins=1@640,150;2@545,780 | 06:11. Destroyed and burning Red tanks on the road north of PL RED <span class="pinref">1</span>; the radio net <span class="pinref">2</span> lists contacts, updates and kills.

You don't need to do anything during this fight: your crews detect, report, distribute fire and shoot. What you *could* do:

- If one platoon can't see the enemy, move it to a better position (but every move exposes you).
- If Red gets close on one side, shift the other platoon's battle position to cover it.
- If things go wrong, **BREAK CONT** (<kbd>X</kbd>) gets a platoon out under smoke.

## Step 6: The end

When Red has lost 70 % of its vehicles it is **combat ineffective** and the scenario ends. A panel offers **AFTER-ACTION REVIEW**, **RESTART**, **MAIN MENU** or **CLOSE** (which lets you keep watching).

::fig g-late | crop=500,120,600,122 | w=75 | The mission-end panel.

## Step 7: Look at what really happened

Open the **AFTER-ACTION REVIEW** (<kbd>F8</kbd>). It shows the ground truth: where every vehicle really was, every shot, and statistics per side. In this run, Blue fired 17 rounds, hit 14 and destroyed 7 tanks without loss. Red was spotted 610 seconds after H-hour (Red had first reported something at 397 s).

::fig g-aar | crop=0,0,1600,900 | pins=1@160,44;2@1430,135;3@640,830 | The after-action review: playback controls <span class="pinref">1</span>, statistics <span class="pinref">2</span>, timeline with event marks <span class="pinref">3</span>. <kbd>Space</kbd> plays, <kbd>←</kbd>/<kbd>→</kbd> jump 10 s, drag the timeline, <kbd>Ctrl</kbd>+<kbd>R</kbd> saves a replay file.

<div class="tip" markdown="1">
<span class="label">What made this easy</span>
You defended, hidden, with better sights (thermal imagers) and better guns, against an enemy that drove down a road in column into your field of fire. The other scenarios take each of those advantages away in turn: night, fog, an enemy that attacks across a broad front, an attack of your own across open ground. Part III explains how to cope.
</div>
