# M8h1b towns: design plan

Draft 2026-10-29. TODO item H3 (settlement interiors, town blocks and streets beyond villages).
Exit (TODO.md): a generated town with streets and enclosed spaces; baker and hash tests;
`BAKER_VERSION` bump; Z4 re-checked; balance re-run if existing maps changed. Decisions are in §7
(agreed 2026-10-29: all recommendations).

## 0. Scope

Tank combat stays the theme. A town matters to the game as ground tanks should not fight in
alone, as LOS blockers along streets, and as a place where squads hold blocks. M8h1b makes the
generator build one; the AI that fights *through* a town block by block stays with M8f2 (F5's
remainder, B37).

Already decided (M8B-INFANTRY §9 #5, DESIGN.md §14 #29): **towns go on a new map**; `demo` and
`uplands` keep their villages. So the v1 balance should not move: the plan keeps the village
code path byte-identical and checks it (§5).

## 1. What exists

- `terragen::roads`: village sites scored for flat open ground, spaced by `village_spacing_m`;
  a road network (MST main roads, RNG secondary links, edge exits) over a slope-penalised grid;
  village buildings placed along road frontage plus loose infill, 9–15 m × 7–11 m, 1–3 storeys,
  85 % masonry, 15 % wood. No streets of their own: the through roads are the streets.
- Baked buildings (M8h1a): id raster, entrances (wall midpoints on open dry ground), building
  classes `wood | masonry | concrete | industrial` in `data/terrain/buildings.ron` with wall
  thickness, storey height, protection and squad capacity. `concrete` and `industrial` exist in
  data but the generator never makes them.
- Cells are 4 m. Building cells are no-go for vehicles (no width clearance in the raster: a
  one-cell gap is "passable"), open at interior speed on foot through entrances.
- Land cover `village` is `built_up`; tank-only platoons route round built-up ground (M8f1),
  formations follow the route by arc length so a column stays in the street (H10).
- Terrain editor: buildings are single features (place, delete); no settlement polygon.

## 2. The town (generator, `terragen::town`)

One town per map where the recipe asks for it, about 1–1.4 km across, at the best site for a
market town: flat ground near the map centre third, at a river crossing if there is one, where
main roads can meet. It is placed **before** the villages, which then keep their spacing from it;
the road network treats it as a hub (all edge exits connect to it). Sources for the layout:
Parish & Müller (2001) and Kelly & McCabe (2006) (TERRAIN.md §10) for street growth; FM 3-06
(2006) *Urban Operations* App. B and FM 90-10-1 (1993) ch. 1 for the urban zones (old core,
core periphery, residential, high-rise, industrial) and their tactical character.

Zones, as rings and sectors around the centre:

| Zone | Streets | Buildings | Tactical character (FM 3-06 App. B) |
|---|---|---|---|
| Old core (r < ~250 m) | organic: grown from the entry roads, bent, 6–8 m wide, a market square | closed perimeter blocks of terraced masonry, 2–4 storeys, around courtyards; a church | narrow streets, short fields of fire, strong buildings |
| Core periphery / residential (to ~550 m) | grid, rotated to the main road, 8–10 m wide | rows and detached houses, 2 storeys, gardens | wider streets, fields of fire down the grid |
| High-rise estate (one sector) | wide access roads, open ground between blocks | concrete slab blocks, 16 m (5 storeys of 3.2 m), 60 × 12 m | long sight lines between blocks, upper-floor positions |
| Industrial (one sector, by the river or main road) | wide yards | industrial sheds, 8–10 m high, 30–60 m footprints | large open buildings, yards |

**Enclosed spaces**: perimeter blocks leave a courtyard inside; one or two 3 m gateways per
block (foot only: narrower than any vehicle) lead into it. Courtyards are open ground for the
foot raster and LOS, enclosed for vehicles.

**Streets** are written as road features of a new class `street` (paved, width from data) so
they draw and drive like town streets rather than main roads, and the authoring package stays
the source of truth (author → bake → run). Vehicle streets are at least 8 m between facades
(two cells) so a raster gap of one cell never reads as a tank-wide street; the gateway rule
above keeps narrow gaps foot-only on purpose.

**Land cover**: a new class `town` (`built_up: true`, `foot_speed_factor` as `village`) under
the whole town outline; courtyards and the square stay `town`.

**Recipe**: `RoadParams` gains `town: Option<TownParams>` (radius, zone shares, block size,
street widths), `serde(default)` so `rolling-farmland` and `wooded-uplands` are unchanged. A new
recipe `market-town` (rolling farmland relief with a town) and a new map `town` (8 km, seed
chosen so the town sits at a river crossing with open approaches).

## 3. Baker and data

- `RoadClass::Street`: mobility speed and display style in data (`data/terrain/landcover.ron`
  road entries or a small street table), width per street from the feature.
- Building heights from the zone: concrete and industrial come in at last. `floors` and
  `squad_capacity` already handle them (a 60 × 12 m five-storey block holds 36 squads by today's
  `squads_per_100m2`; a cap per building may be needed so one block does not swallow a company,
  §7 #5).
- `BAKER_VERSION` 3 → 4 (new road class and land cover change the baked output for the new map;
  old baked maps rebuild). `demo` and `uplands` bake to the same rasters as before (§5).

## 4. Sim and AI (what changes, what does not)

No new behaviour is required for the exit, but the town must not break what exists:

- Built-up avoidance (M8f1) covers `town` through `built_up`; with a 1.2 km town the detour is
  long, which is the intended doctrine (FM 3-06.11 (2002): tanks bypass unless infantry leads).
- Formations in streets: H10 slots by arc length, so a column follows a bending street. Z4 is
  re-checked on the town map (wedge and column through the core and through the grid).
- Squads: `choose_buildings` scores walls, multi-floor and the view along the axis; slab blocks
  will score high. Capacity cap per §7 #5.
- Performance: the town adds roughly 600–1,000 buildings to today's ~100. Watch bake time, the
  foot raster, building choice and sensors (buildings are rasters, so LOS cost is unchanged).
  Budget: bake of the town map under +50 % of `demo`; `tanks-headless bench` LOS mean within
  20 % of `demo`; the town scenario at ≥ 20× real time on the development machine.

Out of scope here: clearing by blocks, sections overwatching intersections (TERRAIN.md §5.4
"AI: ... move in column with sections overwatching intersections") and the 1 m urban surface
layer for LOS (§7 #2), building damage and rubble (D4, M8d), rail.

## 5. Tests

- Generator: the town has streets, ≥ N closed perimeter blocks, every block's courtyard is
  reachable on foot and not by vehicle, every street cell is connected to the road network,
  zone classes appear (concrete, industrial), deterministic (same seed → same features hash).
- **No drift**: `demo` and `uplands` features and baked rasters hash the same as before the
  change (a test pins today's hashes).
- Baker: `street` mobility and land cover `town` built-up; entrances on courtyard sides.
- Sim: a tank platoon ordered across the town routes round it; with BUILT-UP ENTER, in column,
  it reaches the far side within a time bound; squads occupy a slab block's upper floor.
- Golden replay hash: unchanged (the m1 scenario is on `demo`).
- Z4 re-check numbers recorded in TODO.md.

## 6. Content

One scenario on the town map to exercise it (§7 #3), plus screenshots of the town at three zooms
and with the traffic and cover layers.

## 7. Decisions

| # | Question | Recommendation |
|---|---|---|
| 1 | Town zones in the first version | **All four** (old core with closed blocks and courtyards, grid residential, a high-rise estate, an industrial sector): they differ in exactly the things that matter here (street width, sight lines, building strength, height) |
| 2 | 1 m urban surface layer for LOS (TERRAIN.md §5.4) | **Defer**: keep 4 m cells, make vehicle streets ≥ 2 cells wide and test LOS down a street; revisit if street LOS reads wrong in play |
| 3 | The town scenario | **Blue mech team defends the town** (squads in the blocks, tanks on the edges) against a Red commander-AI attack with BMPs and tanks: exercises built-up avoidance, IFV dismounts and building choice on a real town |
| 4 | Town AI beyond what exists (overwatching intersections, block clearing) | **Defer to M8f2** with F5's remainder and B37 |
| 5 | Squad capacity of large buildings | **Cap at 4 squads per building** (a platoon and a bit), in data, so a slab block is a platoon position, not a company |
| 6 | Editing towns | Generator writes streets and buildings as ordinary features (editable one by one today); a **settlement polygon tool that re-fills an area waits for M8j** |

**Agreed 2026-10-29**: all six recommendations (DESIGN.md §14 #35).

## 7a. Results and deviations (2026-10-29)

- **Generator** (`terragen::town`): one lattice of street nodes, 85 m apart, rotated to a random
  axis; bent and jittered (±12 m) inside the 230 m core, fading to regular one block outside;
  the outline wanders ±20 % with bearing. Quads take a zone (square, core, residential,
  high-rise sector 70°, industrial sector 60° opposite it); quads of one block merge (residential
  1 × 2, high-rise and industrial 2 × 2); edges between blocks are the streets, written as
  `street` road features (7 m in the core, 9 m outside). The routed network treats the town as a
  hub (every edge exit runs to it), town ground is expensive to route over, and a road that
  reaches the town is cut at its edge and joined to the nearest street node.
- **Courtyards (deviation)**: no gateways. Courtyards are closed to vehicles and reached on foot
  through the houses (each row house has an entrance on the street and one on the courtyard),
  so the foot-only rule needs no narrow gaps, which would read as tank-wide at 4 m cells.
- **Industrial sector (deviation)**: opposite the high-rise estate at a random bearing, not
  placed by the river or the main road.
- **Streets** bake as `road` land cover on paved soil; built-up avoidance still covers them
  through the 30 m margin round buildings.
- **`town` map** (`market-town` seed 42, 8 km): 65 roads (48 town streets), 1,444 buildings
  (1,243 masonry, 150 wood, 31 industrial, 20 concrete), generated in 3.8 s and baked in 2.0 s
  (`demo`: 3.7 s and 2.0 s). The town is at (4200, 3400), next to the river.
- **No drift**: `demo` and `uplands` regenerate with identical authoring and baked hashes;
  `recipes_without_a_town_generate_as_before` pins a small rolling-farmland map. `BAKER_VERSION`
  4 (old bakes rebuild once). Golden replay hash unchanged. `SIM_VERSION` 7 for the squad cap
  (no shipped building exceeded four squads, but a large hand-made one now plays differently).
- **Tests**: `terragen/tests/town.rs` (streets, zones, determinism; on a 3.2 km map 20 closed
  courtyards, all 20 reachable on foot); `headless/tests/m8h.rs` (on the `town` map: the default
  route has 0 m on built-up ground, BUILT-UP ENTER 1,127 m; a slab block takes 4 of 5 squads).
- **Z4 re-check**: south edge to north edge with BUILT-UP ENTER (1.9 km, 1.2 km of it in town):
  column 262 s, wedge 351 s, no stalls.
- **Performance** (`tanks-headless bench`, which now includes `m8h-town`): LOS at 4 km mean
  29.9 µs (`demo` 29.3), BP search 33 ms, the town scenario at 248× real time. Within the §4
  budget.
- **Scenario** `m8h-town` (THE MARKET TOWN, 1987): Blue M1 platoon and M2 platoon with three
  squads hold the north edge; Red three T-72B platoons and two BMP platoons with squads attack
  from the north through the woods under the commander AI. AI against AI, seeds 1–8: Blue wins
  8/8 (four at the time limit), Blue loses 2–6 of 10, Red 9–15 of 21. A first version with Red
  crossing open farmland from the north-west was a rout (Red combat ineffective in 12 minutes
  even against a Blue with no orders); stronger T-72Bs, a third tank platoon and rain did not
  change that, the wooded approach did. The AI attack still never wins; that is the commander
  AI (M8f2, TODO I10), not the town.
