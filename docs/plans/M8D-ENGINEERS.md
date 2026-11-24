# M8d engineers and obstacles: design plan

Status: **2026-11-24, built except D4** (§8a); all nine recommendations in §8 accepted (DESIGN.md §14 #40). TODO.md M8d: D1–D4, D6; the exit scenario feeds I2. Release `0.1.4` follows the end of M8d (TODO K4).

Goal: ground can be closed by obstacles and mines, opened again by breaching, and crossed at bridges and fords that can be lost. Tanks react to an obstacle as doctrine says, and the cost of crossing (time, exposure, a lane that funnels the attacker) shows up in play. Exit: a deliberate-breach scenario with scenario tests (with breaching assets the attack gets through, without them it stalls or detours), balance batch, golden hash unchanged for every scenario without obstacles.

Doctrine sources to add to `REFERENCES.md` (public US Army manuals; read before coding, cite in doc comments):
- FM 3-34.2 (2000) *Combined Arms Breaching Operations*: SOSRA (suppress, obscure, secure, reduce, assault), breach organisation, lane marking.
- ATP 3-90.4 (2016) *Combined Arms Mobility*: breaching and gap crossing, lane widths, bypass.
- ATP 3-90.8 (2021) *Combined Arms Countermobility Operations* and FM 90-7 (1994) *Combined Arms Obstacle Integration*: obstacle effects (disrupt, turn, fix, block), obstacle groups covered by fire.
- FM 20-32 (1998) *Mine/Countermine Operations*: minefield types, density, probability of encounter, mine plough and roller use.
- FM 5-34 (various editions) *Engineer Field Data*: military load classification (MLC), trench and gap widths, obstacle dimensions.
- ATP 3-20.15 (2025) / FM 3-20.15 (2007) tank platoon: battle drill *react to obstacle* (for the platoon drill below).
Numbers we cannot source get `estimate` with reasoning, as everywhere else.

## 0. Scope

In: static obstacles placed by the scenario (ditch, wire, abatis/log crib, dragon's teeth / wall, minefield), detection of obstacles, the react-to-obstacle drill, breaching by tank-mounted plough and roller and by engineer squads, lanes, explicit bridges with MLC and fords, bridge demolition, editor placement. D4 (building damage, rubble, tanks through light walls) is proposed as its own slice at the end (decision #6).

Out (later or never in M8d): obstacles built during the battle (digging a ditch, laying a field in contact), scatterable mines (artillery or air delivered), MICLIC-type line charges and armoured vehicle-launched bridges (decision #4), counter-obstacle reconnaissance as its own task (E-phase), mine strike on dismounted infantry (anti-personnel mines), and any restricted data. Tank combat stays the theme (M8B-INFANTRY.md §0): engineers are abstracted to what a tank company commander sees.

## 1. What exists

- Vehicles already carry `combat_mass_kg`, `fording_depth_m`, `vertical_step_m`, `trench_width_m` (`sim-data` vehicle mobility), unused for obstacles so far.
- Baked map: per-class planning speed rasters (`MobilityLayer.speed`), `terrain_cap` in vehicle physics from land cover and water depth, roads over rivers as implicit bridges (`bake.rs`, DESIGN.md §14 #11).
- Runtime overlays on the planning rasters already exist for built-up ground (`sim::builtup::AvoidLayers`, per side and vehicle type) and the client can read them (`Sim::planning_speed`, B35). Obstacles follow the same pattern.
- Infantry squads (move, dismount, occupy, suppression), the plan runner with CP actions, branch conditions and contact reactions, seeded per-subsystem RNG, supply points as a scenario-layer precedent (scenario item, editor placement, picture view).
- `Condition::MobilityKill`, recovery vehicles (M8c) — mine strikes feed straight into them.

## 2. Obstacles (D1)

**Representation: a scenario layer, not terrain** (decision #1). `ScenarioDef.obstacles: Vec<ObstacleDef>` with `kind`, `side` (owner), a polyline or polygon, and kind parameters (ditch width and depth, wire rows, minefield density and mine type). Obstacle kinds live in `data/combat/obstacles.ron` with sourced dimensions: what each stops (vehicle crossing rule), what it slows (foot), its LOS height (a berm or wall blocks; wire does not), and its breaching time per method.

**Crossing rule per vehicle**, from data the vehicles already have: a ditch is crossable if its width ≤ `trench_width_m` and its side ≤ `vertical_step_m`, else no-go; a wall/teeth line is no-go to vehicles; wire slows wheeled and dismounted movement, tracked vehicles cross it (it fouls running gear: delay from data); abatis no-go to vehicles until reduced. Physics and planning both apply it:
- *Physics*: an obstacle raster (cell → obstacle id, rasterised at load at the map's 4 m cells) is checked in `mobility.rs` next to `terrain_cap`; a vehicle entering a no-go obstacle cell stops at its edge (as at a no-go slope today).
- *Planning*: per side, a copy of each vehicle type's speed raster with the obstacles that side *knows* written in (no-go or slowed), built at load and rebuilt when the side learns of an obstacle or a lane opens. Same structure as `AvoidLayers`; the built-up overlay is applied on top.

**Fog of war** (decision #2): a side knows its own obstacles from the start. Enemy obstacles enter its picture when seen: a sensor check against the obstacle's signature (ditches, walls, wire and surface-laid mines visible in daylight at ranges from data; buried mines only by a strike or by engineers within a short range). The picture gets `ObstacleView { id, kind, geometry, lanes, known_since }`; planning for that side reads only those. An unknown obstacle is found by running into it: the vehicle stops, the obstacle becomes known, and the drill starts.

**React to obstacle** (ATP 3-20.15 battle drill; like every new AI drill: a default, a per-leg SOP setting and a branch condition, as ON CONTACT and ON AMBUSH do): halt, report (`1: OBSTACLE, WIRE AND MINES, GRID 0412 0388.`), go to overwatch, then by the leg's ON OBSTACLE setting: **BYPASS** (default: re-plan round the known obstacle if a route exists within a data detour factor, else wait), **BREACH** (if the platoon has a plough or roller, or engineer squads are attached), **HALT** (wait for orders). Branch condition `OBSTACLE` so a plan can switch to a breach or a bypass route. Commander AI: attack task bypasses or breaches by the same rule; defend task needs no change beyond positions that cover the obstacle (scenario authored).

## 3. Minefields and breaching (D2)

**Minefields** are obstacles with a mine type (`data/combat/mines.ron`: fuze class — pressure (track-width) or full-width (tilt-rod/magnetic); effect table per vehicle armour class: mobility kill, firepower kill, catastrophic, all `estimate` unless a public source gives shares) and a density. A vehicle crossing a minefield rolls a strike per metre travelled inside it, the rate from density and the vehicle's track or hull width (the probability-of-encounter approach of FM 20-32; the formula and its constants get a unit test with the source's reference values). One seeded `ChaCha` stream for mines (CLAUDE.md rule 2). A strike stops the vehicle (`MobilityKill` or worse), reports (`1-2: MINE STRIKE. IMMOBILISED.`), and makes the minefield known to the side.

**Breaching assets** (decision #4):
- *Mine plough / roller*: a vehicle attachment (`UnitDef.attachments: ["plough"]`, data in `data/vehicles/attachments.ron`: in-service dates, which vehicles take it, speed while ploughing, clearance probability per mine type, lane width; plough and roller trade speed against reliability). A tank with a plough reduces a lane through a minefield at plough speed; a roller detonates pressure mines ahead of the tracks and is worn by each detonation.
- *Engineer squad*: a squad type (`data/infantry/`, era-dated) that reduces wire, abatis and a minefield lane by hand at a slow data rate, and can fill a ditch section only with time (or not at all: decision #4). Uses the existing squad framework (dismount, move on foot, suppression stops work).

**Breach drill** (FM 3-34.2 SOSRA): plan action **BREACH** at a CP facing the obstacle, for a platoon with a breaching asset. The platoon (or its plough tank) moves to the point of breach, reduces a lane, marks it, reports `LANE OPEN`. Suppression and obscuration are the player's: overwatch from another platoon, smoke from the existing smoke tools, or a fire mission; the AI breach team waits under fire above a suppression threshold (data). A lane is a strip of the obstacle opened for vehicles (width from data), drawn on the map, added to the planning rasters for both sides once seen. Traffic through a lane is column, one vehicle at a time (formation collapse already exists for narrow streets, `route_slot`).

## 4. Bridges, fords and demolition (D3)

**Explicit bridges** (decision #5): authoring feature `bridges` in the GeoJSON (TERRAIN.md §5.2): deck line, width, MLC, construction class. The baker writes them as passage cells over water for vehicles whose MLC fits, and marks them in a `bridge_id` raster; roads crossing water without an explicit bridge keep today's implicit bridge (decision #11 unchanged), with a default MLC from data so old maps keep working. `BAKER_VERSION` bump. Vehicle MLC from combat mass (the public rule of thumb that a tracked vehicle's class is close to its weight in short tons, refined by FM 5-34 tables if available; source checked before coding). A vehicle over capacity treats the bridge as no-go. The generator adds explicit bridges where its routed roads cross rivers.

**Fords**: river vertices already carry depth and bottom type; a ford is a river segment below the fording depth of most vehicles, labelled on the map with its depth and bottom. No new feature type unless review asks for one.

**Demolition**: a bridge prepared for demolition is a scenario item (`ScenarioDef.demolitions`: bridge id, owning side, state `Prepared`); it is blown by a plan action, a trigger, or a code word. The bridge cells become water again in the runtime obstacle layer (the baked map stays immutable), the planning rasters of every side that sees it are rebuilt, vehicles on the deck are destroyed. The AI treats bridges as chokepoints: the defence may blow a bridge when the enemy is within a doctrine range of it (doctrine pack, `estimate`); the attack plans an alternative crossing.

## 5. Building damage and rubble (D4, last slice)

HE and main-gun hits on a building add damage against a per-class threshold (`data/terrain/buildings.ron`); past it the building collapses to rubble: its cells become slow-go for vehicles and foot, its height drops for LOS, its squads take losses and leave. Tanks may push through `wood` buildings (and later light masonry) at a cost in speed and a chance of throwing a track. This needs a runtime height overlay on `Ground` for LOS, which touches the hottest code path (LOS benchmarks, Z5): measure before and after. Proposed as the last slice, or split out of M8d if the LOS cost is high (decision #6).

## 6. Determinism, hash, replays

- New state (obstacle lanes, known obstacles per side, mine strikes, bridge states, building damage) is hashed; all of it is empty when a scenario has no obstacles, so the golden hash and every existing scenario's balance are unchanged. The commit that first adds hashed state says so.
- Mines use their own seeded stream; obstacle rasterisation is integer cell geometry in a fixed order.
- New `Command` variants and scenario fields are additive (`#[serde(default)]`), so old scenarios and replays load unchanged. A bake format change bumps `BAKER_VERSION` (maps regenerate).

## 7. Picture and UI

- APP-6D / MIL-STD-2525D obstacle graphics: minefield (with the mine type marker), AT ditch, wire, abatis, wall/teeth, lane (with markers), bridge with its MLC number, ford with depth; enemy obstacles drawn once known, with the time seen. Colour is never the only cue.
- Command card / context menu: BREACH (with a breaching asset selected), plan action BREACH at a CP, leg setting ON OBSTACLE (BYPASS / BREACH / HALT), branch condition OBSTACLE, demolition FIRE (code word or now).
- Radio: obstacle reports, mine strikes, `LANE OPEN`, `BRIDGE BLOWN`, terse brevity as elsewhere.
- GO/NO-GO layer shows known obstacles; status card shows the attachment (`PLOUGH`).
- DATABASE: obstacle kinds, mine types, attachments, MLC per vehicle, each with source and confidence.
- Scenario editor (D6): an OBSTACLES page (kind, side, mine type, density), polyline/polygon placement like phase lines, attachments per unit, bridges prepared for demolition; problem list checks (attachment not in service, plough on a vehicle that cannot take it, obstacle off the map).

## 8. Decisions (recommendations in bold)

| # | Question | Recommendation |
|---|---|---|
| 1 | Obstacles in the scenario or in the terrain package? | **Scenario layer** (`ScenarioDef.obstacles`), like supply points: they belong to a side, change at run time (lanes) and are a scenario designer's tool. Permanent man-made features (walls, embankments) stay terrain; an authored terrain obstacle layer can come with H1 import if wanted |
| 2 | Does a side plan round obstacles it has not seen? | **No: only known obstacles** enter its planning rasters (rule 3, fog of war); unknown ones are found by contact or observation and trigger the drill |
| 3 | Default reaction to an obstacle | **BYPASS** where a detour exists within a data factor, else halt in overwatch and report; per-leg ON OBSTACLE setting and an OBSTACLE branch condition (standing rule: AI default + SOP + branch) |
| 4 | Breaching assets in M8d | **Plough and roller kits on tanks, plus an engineer squad** (hand breaching of wire, abatis and a mine lane; no ditch filling). Line charges (MICLIC class), armoured vehicle-launched bridges and combat engineer vehicles go to the TODO for a later slice |
| 5 | Explicit bridges in the bake | **Yes**, an authoring `bridges` feature with MLC (BAKER_VERSION bump); roads over water without one stay implicit bridges with a default MLC, so old maps and decision #11 keep working |
| 6 | D4 (building damage, rubble, push-through) in M8d? | **Last slice of M8d, only if the LOS height overlay costs < 10 % on the Z5 benchmark**; otherwise move it to M8h2 or ENG and close M8d without it |
| 7 | Mine effects: random or fixed? | **Random per metre in the field from one seeded stream** (FM 20-32 encounter model); effect shares per mine type from data (`estimate` where no public figure) |
| 8 | Obstacles built during the battle | **Out of scope** for M8d (TODO entry): every obstacle is placed by the scenario |
| 9 | Exit scenario | **A deliberate breach on `uplands`**: a Blue tank company with two plough tanks and an engineer squad attacks through a covered obstacle belt (wire, minefield, ditch) held by a Red platoon; plus Red bridge demolition on a second scenario if D3 lands. Tests: with breaching assets Blue opens a lane within a set time and seizes; without them it bypasses or stalls |

## 8a. Results and deviations (2026-11-24)

- Built as planned: data (§1 sources read; FM 20-32 calibrates the encounter model), obstacle layer and per-side rasters, sighting, the drill, minefields, plough / roller / engineer breaching, client, scenario editor page, bridges with classes and demolitions, exit scenario. Results: DESIGN.md §13.1 M8d.
- The CP action BREACH became the leg setting ON OBSTACLE: BREACH (same effect at the next obstacle on that leg, and it also works for a platoon told now).
- A platoon with a plough or roller, or of engineers, defaults to BREACH (the breach force); every other platoon to BYPASS.
- Moves plan round known obstacles only while the detour is acceptable (2.5x; 1.2x to reach a known lane when set to BREACH), else straight at them for the drill, so a known belt with no way round is breached or reported, not "NO ROUTE".
- Followers in a plough or roller lane meet a tenth of the mines the kit missed (`lane_follow_factor`, estimate); lane width is at least 3 m either side in planning and physics alike, so a vehicle routed through lane cells is in the lane.
- Generated bridges all get class 80 (no balance change); classes by road, fords, a bridge tool and demolitions in the editor are TODO D7-D9. Engineers breach only as a dismount platoon (D8).
- D4 (building damage, rubble, push-through) was not attempted: the LOS height overlay was not built or measured (TODO D4).

## 9. Order of work (each about one commit)

1. Sources read and added to `REFERENCES.md`; data files `obstacles.ron`, `mines.ron`, `attachments.ron` with sources and confidence; loader tests; DATABASE rows.
2. Sim: obstacle layer (rasterise, physics check), per-side planning overlays, `ObstacleView` in the picture; tests on a synthetic map (a ditch wider than the trench width stops a tank; a route goes round a known wall).
3. Detection of obstacles and the react-to-obstacle drill (BYPASS / BREACH / HALT), leg setting, OBSTACLE branch; scenario tests.
4. Minefields: strike model (unit test against the source's reference values), mine stream, reports, recovery interplay.
5. Breaching: plough and roller attachments, lanes, BREACH action and drill; engineer squad type and hand breaching.
6. Client: obstacle and lane graphics, BREACH, ON OBSTACLE, radio, GO/NO-GO, status; screenshots.
7. D6 scenario editor OBSTACLES page and attachments.
8. D3 explicit bridges (baker, generator, MLC), fords labelled, demolition (scenario item, action, AI rule).
9. Exit scenario(s), scenario tests, balance batch, DESIGN.md §13.1 and §14, TODO ticks, guide chapter.
10. D4 (if decision #6 keeps it): building damage, rubble, LOS height overlay with benchmarks, push-through.
