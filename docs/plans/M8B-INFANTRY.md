# M8b infantry and M8h1a buildings: design plan (draft for review)

Status: **decisions agreed 2026-10-24** (§9, recommendations accepted, plus the scope rule in §0). TODO.md Z6 is closed; M8h1a and M8b-a may start in the order in §10. Covers M8h1a (H7–H10) and M8b-a (B1, B3–B5, B7, B8, B10, B11, B14–B18); M8b-b (anti-tank teams) is sketched only where it shapes the core.

Goal of the two phases together: an IFV platoon dismounts its squads by player command, the squads walk into a village and occupy a building, tanks and infantry can see and fight each other with sourced numbers, and every result replays to the same hash.

## 0. Scope rule (agreed 2026-10-24)

**Tank combat is the major theme of the game.** Infantry is abstracted: it exists to make armour decisions matter (dismounts in villages, ambushes, tanks that cannot clear a building without HE), not to be simulated in detail. Concretely:

- A **dismounted squad is one abstract element**: one strength number, one of three postures (moving, halted, dug in or occupying), one burst-fire model, one signature row per posture. No fire teams, individual men, per-man visibility, or per-man movement in v1.
- **Mounted squads** matter only at carrier level: they ride with the IFV, count toward its capacity, and are lost or wounded with it by a data table. Nothing about them is simulated while they are inside.
- Infantry AI stays small: follow a foot route, take the posture for the situation, seek cover and return fire on contact, occupy a building. Drills are cited, but only the ones the dismount/remount and ambush scenarios need.
- Building work (H7–H9) covers only what squads need from buildings. Narrow-street column AI (H10) is already done (2026-10-21, TODO.md) and needs no further work for infantry.
- When in doubt, prefer the simpler model and record the cut in §14 of DESIGN.md. Fidelity that does not change a tank decision is out of scope.

Anything in the sections below that goes beyond this rule should be cut or simplified, not built.

## 1. Scale and granularity

- **One entity = one squad** (7–9 men), not individual soldiers or fire teams. The player commands a company team (DESIGN.md §14 #3); squads are the smallest element the company commander deals with, as vehicles are. Fire teams can come later as a split order without changing the model.
- A squad carries a **strength** (men fit to fight) and loses men to hits. It is **combat-ineffective** below a doctrine threshold (data, default 50 %: it stops manoeuvring, keeps defending, and counts as lost for victory) and **destroyed** at zero.
- Expected numbers: a mechanised company team has 3 IFV platoons × 3–4 squads at most, so ≤ 12 squads per side, about 60 entities in a large scenario. Z5 sets the budget against that.

## 2. Entity model (B1)

`World` already has optional components (DESIGN.md §4.4, `world.rs`). Infantry is **additive**:

| Component | Vehicle | Squad | Notes |
|---|---|---|---|
| `transform`, `side`, `callsign`, `condition`, `platoon`, `suppression`, `weapons` | yes | yes | unchanged |
| `vehicle: Option<usize>` | Some | **None** | most uses already skip `None` (`let Some(vi) = … else`); the two `expect("vehicle")` calls (`platoon_ai.rs` speed limit, `gunnery.rs` target def) must handle squads |
| `mobility` | Some | **None** | squads use a new foot movement state |
| `sensors` | Some | Some | same `SensorState`, sensors from the squad def |
| `gun` | Some | **None** | squads use `squad_weapons` |
| new `squad: Vec<Option<SquadState>>` | None | Some | def index, strength, posture, foot movement, carried weapons with stock, `mounted_in: Option<EntityId>`, `in_building: Option<BuildingId>` |
| new `carrier: Vec<Option<CarrierState>>` | IFVs | None | capacity (from the vehicle def), embarked squad ids, ramp state |

- **Mounted squads exist as entities from the start** (scenario lists them with `mounted_in: "<carrier callsign>"`). While mounted they have no own position (they follow the carrier), are skipped by sensors, LOS and the picture, and die or are wounded with their carrier by a data table (catastrophic kill → all lost; penetration without fire → a fraction). Stable ids, no spawn/despawn, one hash layout. *Alternative: spawn on dismount; rejected because ids, platoon membership and replays get harder.*
- New data: `data/infantry/*.ron` → `SquadDef` (name mirroring a real organisation, in-service dates, men, weapons, sensors, posture signatures, foot speeds, crossing limits); `VehicleDef` gains optional `troops: { capacity, mount_s, dismount_s }` (sourced: ramp/door drill times). Era is data: the squad type in a scenario is picked like vehicles.
- Order: a new `Order::Foot { route, … }` and `Order::Occupy { building }`, `Order::Mount { carrier }`; the existing `Move`/`Face`/`Reverse` stay vehicle-only. Squad AI runs in the vehicle AI phase (5 Hz) as its own function, not branches inside vehicle code.

## 3. Detection (B10, B8)

Sensors today read target size from `VehicleDef.dimensions` and skip targets without one (`sensors.rs:139`). Change: a `Signature { width_m, length_m, height_m, hull_height_m, thermal, motion }` is computed per target from either the vehicle def or the squad def + posture; Johnson/ACQUIRE stay as they are (DESIGN.md §6.1).

| Posture | Critical size (data, estimate) | Notes |
|---|---|---|
| Moving / standing | ~0.5 × 1.8 m per man, squad as a group of 2–3 visible men | motion cue strong |
| Halted, kneeling / prone | ~0.5 × 0.5 m / 0.5 × 0.3 m | |
| Dug in | ~0.4 × 0.25 m | after a dig-in time |
| In building | window-sized, ~0.6 × 0.8 m | only through the facade facing the observer; firing reveals |

- Thermal: people are high-contrast against vegetation; crossover affects them less than vehicles (data multiplier, `estimate` with a cited reasoning).
- Firing reveals (small-arms flash and noise, RPG back-blast in M8b-b) through the existing `last_fired` cue.
- **Tall crops** (B8): crop land cover in season counts as concealment for prone/kneeling postures only (a canopy-like attenuation with low height), not for vehicles.
- Picture: `ContactLevel` stays; contacts gain a **kind** (`Vehicle | Infantry`) known at `Recognised`. `Identified` infantry gives the type (rifle squad, AT team). Infantry contacts are small symbols in the picture, the roster and the AAR (B15).
- Deliverable: `tanks-headless probe` CSV of time-to-detect vs range by posture and sensor (exit criterion).

## 4. Weapons and effects (B3, B4, B5, B11)

Two resolution models, both seeded and hashed:

1. **Direct-fire projectiles (existing)** for anything aimed at vehicles: main gun, autocannon AP, ATGM, and in M8b-b the RPG. Unchanged.
2. **Burst fire vs. personnel (new)**: MG (coax, squad MGs), rifles, autocannon HE/HEI, main-gun HE-FRAG/canister against squads. One burst = one draw: expected casualties = `rate × hit chance(range, posture, visible fraction, suppression of shooter) × lethality(posture, cover)`, sampled per man; plus suppression on the target (reusing the M7d suppression model, B5). Area effect of HE reuses `HeEffects` with a new `Infantry` target class (fires.rs; per posture, and per building class when occupied).

- **Coax MG** (B3): a second weapon on tanks/IFVs with its own stock, fired by the gunner at infantry targets; does not need the main-gun cycle. Data per vehicle.
- **Tank gunnery vs infantry** (B11): target class `Infantry` in `select_round` (`preferred_vs`): HE-FRAG / canister / 25–30 mm HE by preference, coax at short range; the contact drill engages infantry only when weapons status allows and it is the most dangerous threat (doctrine pack).
- **Small arms vs vehicles**: no penetration of armour; they force an unbuttoned commander down (sensor degradation while under fire) and add suppression. Heavy MG (12.7 mm) can kill light vehicles: deferred unless a source-backed table is cheap.
- Building protection (from H7) multiplies lethality and suppression for occupants.

Sources to add to `REFERENCES.md`: FM 3-22.68 *Crew-Served Machine Guns* (beaten zone, rates), FM 90-10-1 *An Infantryman's Guide to Combat in Built-up Areas* (1993, wall protection vs. weapons), ATP 3-21.8 *Infantry Platoon and Squad* (2016) and FM 7-7J (1986) / FM 3-21.71 (2002) *Mechanized Infantry Platoon and Squad (Bradley)*, FM 100-2-3 (1991) for Soviet-style squads. All public; numbers we cannot find are `estimate` with reasoning.

## 5. Buildings (M8h1a: H7–H10)

Today: building footprints are authored features with `height_m` and a `BuildingClass` (`Wood | Masonry | Concrete | Industrial`); the baker rasterises only a height grid (`buildings.bin`), LOS is opaque to roof height and buildings are NO-GO for all classes (DESIGN.md §14 #15).

- **H7 building types**: `data/terrain/buildings.ron` per class: wall thickness, floors (from height), protection factor vs small arms / HE fragments / blast, capacity in squads per 100 m² of floor, and whether tanks can push through (D4 later). Sourced from FM 90-10-1 tables where possible.
- **H8 identity**: baked map gains a `building_id.bin` raster (u32, 0 = none) plus `buildings.ron` in the baked package: id, class, height, footprint centroid, floor area, entrances (footprint edges facing a street or open ground). Sim keeps occupancy per building (squads inside), hashed.
- **H9 foot mobility**: `MobilityClass::Foot` and its own planning speed raster: slope limit (~ 35–45°, data), forest slow but passable, deep water NO-GO, buildings passable only through entrances (route ends inside the building). Foot speeds and rates from ATP 3-21.8 (data). `MobilityClass` stops deriving only from `VehicleDef`.
- **H10 narrow streets** (done 2026-10-21, see TODO.md): a baked `built_up` raster (building density within ~25 m above a threshold). Platoon AI collapses any formation to column in it; leaves column on exit. This changes the AI in today's villages (golden hash, balance re-run).
- Each of H7–H10 changes baked output: one `BAKER_VERSION` bump per commit that changes output, baker determinism test extended (fresh bake == loaded bake).

**Occupancy (B7)**: a squad ordered to occupy a building walks to an entrance, then is "in" it: its position is the building, it observes and fires from the facade facing each target (eye height = a floor height), it is visible only through that facade, and its protection comes from H7. Capacity limits how many squads fit.

## 6. Orders, AI and control (B14, B16, B17)

- Dismounts of an IFV platoon form a **linked dismount platoon** (`1-D` for platoon 1) with its own roster entry, orders and plan; the carrier platoon keeps its own. This keeps "one plan per platoon" (DESIGN.md §14 #26) and lets the player move them apart.
- **B14 minimal commands**: `Command::Dismount { platoon }` (squads exit at the carriers, take a security posture 15–25 m around them; drill time from data) and `Command::Mount { platoon }` (squads walk to their carriers, which halt). Hotkey and one button on the command card; the full plan actions, menus and editor placement are G8/G9.
- Squad AI (5 Hz, `ai` pure functions): follow a foot route; take a posture (prone when halted under fire, kneeling at halts, moving otherwise); react to direct fire (seek cover within ~50 m, return fire: ATP 3-21.8 battle drill "React to contact"); occupy building.
- Doctrine packs gain an `infantry` block (dismount distance from the objective, security posture radius, react-to-contact parameters) in `western-combined` and `generic-opfor`, each with citations.
- **B17**: a BMP-1-class IFV (`Cold War` Red) so Cold War scenarios have dismounts. Data only, sourced (Zaloga, FM 100-2-3), duel harness re-run.

## 7. Client (B15)

APP-6D infantry symbols (crossed frame; anti-armour modifier for AT teams; squad echelon mark), small contact symbols, posture shown in the status card, strength as men left (e.g. `7/9`), a DISMOUNT/MOUNT button, roster entries for dismount platoons, AAR kill credit per squad, database sheets for squad types (weapons, signature table, source/confidence). Client stays thin: reads `PerceivedWorld` only.

## 8. Replays, saves and the hash (B18)

- The golden hash changes deliberately in every commit that adds hashed state (squads, occupancy, coax stock) or AI behaviour (H10 column in villages). Each commit says why.
- `Replay` gains `sim_version: u32` (`#[serde(default)]` = 0). The sim has a `SIM_VERSION` constant bumped when behaviour changes. A replay with another version is **rejected cleanly** by `tanks-headless replay` and the client ("replay from sim version N; this build is M") instead of failing on a hash mismatch. Old replays (no field) read as version 0 and are rejected with the same message. Scenario files stay backward compatible (new fields optional).
- AAR is recorded live, so it is not affected.

## 9. Decisions (agreed 2026-10-24)

| # | Question | Decision |
|---|---|---|
| 1 | Squad as one entity with strength, or fire teams / individuals? | **One entity per squad** with strength; fire-team split later as an order |
| 2 | Mounted squads: entities from the start, or spawned on dismount? | **Entities from the start**, skipped while mounted |
| 3 | Small arms vs personnel: per-burst expected-casualty draw, or per-round ballistics? | **Per burst** (per-round is too expensive and needs data we do not have) |
| 4 | Dismounts as their own linked platoon, or members of the IFV platoon? | **Own linked platoon** (`1-D`) |
| 5 | Towns (H3): new recipe and map, or change `demo`/`uplands` and re-balance? | **New map** for towns; existing maps keep villages (H7–H10 still change them, so Z1's batch is re-run once after H10 if H10 is done) |
| 6 | Old replays after the change? | **Reject cleanly** with a version message; no migration |
| 7 | Do squads use the radio delay and report like vehicles? | **Yes**, same reporting path; dismounted squads may lose position-report quality (E6 later) |

Consequence of §0 (added at review): none for H10, which was already done on 2026-10-21 (narrow-street column AI, TODO.md). Only H7–H9 remain in M8h1a.

## 10. Order of work (each line is about one reviewable commit)

M8h1a:
1. H8 building ids + baked building list (`BAKER_VERSION` bump, determinism test). **Done 2026-10-24.**
2. H7 building class data table and protection lookup (data + unit tests). **Done 2026-10-24.**
3. H9 foot mobility class and raster, `Pathfinder` for foot; test: a foot route through a village ends inside a building. **Done 2026-10-24** (no new pathfinder: the foot raster encodes walls and entrances, DESIGN.md §14 #30).
4. H10 is already done (2026-10-21). Nothing to do here. H7–H9 left the vehicle rasters unchanged (golden hash unchanged), so the Z1 balance batch was not re-run.

M8b-a:
5. Data: `SquadDef`, Cold War and modern squads, carrier `troops` block, BMP-1 class (B16, B17); loader tests.
6. Entity: `SquadState`, `CarrierState`, scenario `mounted_in`, hash; replay `sim_version` (B1 part, B18).
7. Mount/dismount commands and drill timing, foot movement, squad AI posture (B1, B14); scenario test: IFV platoon dismounts and remounts within the drill time.
8. Signatures and detection by posture, contact kind, crops (B10, B8); probe CSV.
9. Burst model, coax, HE vs infantry, suppression of dismounts (B3, B4, B5); unit tests per pairing with cited reference values.
10. Gunnery target class infantry and contact-drill rules (B11); scenario test: tank platoon defeats a squad in the open, fails to clear a masonry building without HE.
11. Building occupancy (B7) and protection.
12. Client: symbols, roster, card button, status, AAR, database (B15); screenshot checks.
13. Golden hash, balance batch and duel re-run; DESIGN.md §13.1 and §14 entries; TODO ticks.

## 11. Tests (exit criteria from TODO.md)

- Unit: signature per posture; burst casualties vs posture and cover against hand-computed values; building protection lookup; foot raster costs.
- Scenario (synthetic maps, explicit assertions): dismount/remount timing; squad occupies a building; tank vs squad in the open and in a building; squad vs squad.
- Determinism: golden replay hash updated; `headless/tests/scenarios.rs` double-run hashes still equal.
- CSV: infantry time-to-detect vs range by posture.
- No leaks: squad AI and UI read `PerceivedWorld` only (type-enforced as today).
