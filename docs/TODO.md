# Master to-do list

The single place to track what is not done. Last consolidated 2026-10-21 from: `DESIGN.md` §2.2, §9.5, §11, §13, §13.1 (Deferred lists), §13.2; `TERRAIN.md` §5, §7–9; the archived plans; the user guide's stated limits; the retired `KNOWN-ISSUES.md`. No `TODO`/`FIXME` markers exist in code.

How to use: items have an id (phase letter + number); each item lives in one place only (M8-0 items keep their old id in brackets). Tick `[x]` when done and say so in the commit; when a phase ends, update `DESIGN.md` §13.1 and move finished items to the "Done" note at the bottom. Add new deferrals here first, not into scattered "Deferred" lines. Nothing below is scheduled until the order is agreed; **bold** = suggested next.

Rules for every item (CLAUDE.md): data in `data/**/*.ron` with source and confidence, no `if era`, sim stays graphics-agnostic, golden hash changes only deliberately, balance batches/duel harness re-run when tuning moves, docs updated.

## Waiting on the user (decisions; check here first)
Added 2026-10-21. Remove a line when it is decided, and record the decision on the item it belongs to.
- **Z6 infantry and buildings plan**: review `docs/plans/M8B-INFANTRY.md` §9 (7 decisions with recommendations). Blocks M8h1a and M8b.
- **I9 dusk attack balance**: keep `mod-dusk-attack` harder at dusk (Blue's passive AI loses about 1 in 3: 5 of 16 seeds) or retune it.
- **M8c defaults**: `docs/plans/M8C-LOGISTICS.md` §6 (supply points as scenario items, transfer at data rates, re-arm events as their own list, repaired vehicles return). Work continues on these defaults unless changed.
- **Z3 release check**: tag `v0.1.0` pushed 2026-10-21; look at the GitHub Actions "Release" run (Windows and macOS legs run for the first time) and the draft of the release page. The repository is private, so the session cannot see the run: paste failing logs to fix them.

## Phase map (v1.x = M8, v1.5 = M9, then v2+)

| Phase | Theme | Depends on |
|---|---|---|
| M8-0 | Housekeeping and prerequisites (do first, small) | none |
| M8h1a | Buildings for infantry: attributes, identity, foot mobility, narrow streets (existing villages) | M8-0 |
| M8b-a | Infantry core: squads, mount/dismount, detection, tank gunnery vs infantry | M8h1a |
| M8b-b | Anti-tank teams: ATGM/RPG, close-in vulnerability | M8b-a |
| M8f1 / M8g1 | AI vs infantry, use of buildings; full mount/dismount controls, editor placement | M8b-b |
| M8h1b | Towns: town blocks, streets, settlement interiors | M8f1 |
| M8c | Logistics: resupply, recovery | none (placed here by priority) |
| M8d | Engineers, obstacles, minefields, bridges, destruction | M8b, M8c optional |
| M8e | Recon, UAS, communications, identification | M8b |
| M8f2 | Commander AI proper (COA, OPFOR plans, auto-commander, fire-support AI, cross/depth fire, hides) | M8b–e |
| M8g2 | Remaining plans and control UX | M8f2 |
| M8h2 | Real-world import, generator, seasons | independent; before big map content |
| M8i | Content and scenarios (ongoing, after each capability phase) | the phase each scenario exercises |
| M8j | Shell, editors, AAR, accessibility | independent; fill gaps |
| M8k | Release and platform | first tag in M8-0 (Z3); rest independent |
| ENG | Engine, performance, fidelity backlog | N2/N6 in M8-0, rest as touched |
| M9 | Tier 3 speculative systems | M8 |
| V2+ | Campaign, multiplayer, aviation, optional extras | later |

**Order:** M8-0 → M8h1a → M8b-a → M8b-b → M8f1 + M8g1 → M8h1b → M8c → M8d → M8e → M8f2 → M8g2. M8h2, M8i, M8j, M8k interleave (M8i after each capability phase).

Why:
- Only the building data infantry needs comes before infantry (M8h1a, on the existing villages); full towns (M8h1b) wait until infantry and the building AI give real requirements. Infantry is a built-up-area problem, but the first ambush plays fine in today's villages.
- M8b is split so the infantry core can be tested before anti-tank teams; a minimal dismount command ships with it so it is playable at once.
- The B13 ambush needs the AI reaction (F11) and full controls, so it is the exit of M8f1/M8g1, not of M8b.
- High-level commander AI last, once every unit type it must reason about exists.
- H3 (towns) changes generator output: either put towns on a new recipe/map or re-run the balance batch (Z1) afterwards; decide in Z6.

## M8-0 — Housekeeping and prerequisites (do first, in this order for Z1–Z3)
- [x] Z1 [I7] Balance re-run on seed-7 uplands after the seed fix. **Done 2026-10-21** (`run --summary`, seeds 1–4, full scenario length): the four uplands scenarios hold (Blue wins 16/16, 2–5 lost except `cw-hasty-attack` seed 4 with 0 lost). The `demo` scenarios, unaffected by the seed fix, drift from the M7e tuning note: see I8. `run` now defaults to the time limit + 60 s (a cut-off at the limit stopped one tick early and read as TimeOut)
- [x] Z2 [J8] Screenshot check of the screens changed on 2026-10-21 (scenario select, database, ADJUST tooltip). **Done 2026-10-21**: all readable, nothing clipped; fixed the two findings (J with no battery gave the no-bursts radio text; a 3750 m maximum range read 3.8 KM)
- [ ] Z3 [K1, K3] Version string on the title screen (**done 2026-10-21**: footer shows `0.1.0` for the tagged build, the `git describe` id otherwise, `DEV` for a plain cargo build; `release.yml` fails a tag that does not match the Cargo version), then **cut the first tag** (tag `v0.1.0` pushed 2026-10-21; tick when the Release run is green on all three OSes) to run the untested Windows and macOS legs of `release.yml` (§13.1 packaging); tag after Z1–Z2 so it is a checked, balanced build
- [x] Z4 [F9] **Re-check**: wedge routed through a village spends minutes re-planning trailing slots. **Re-checked 2026-10-21**: not reproduced as minutes. A platoon ordered through the village centre (CP) and 700 m beyond: `demo` (101 buildings) wedge/column/line/vee 263–284 s; `uplands` (82 buildings) wedge 295 s, **column 398 s**, the platoon fully halted ≤ 8 s in every run. The smaller effect is real: trailing vehicles detour 100–300 m around buildings and the leader waits for laggards (`max_lag` rule). Likely cause: slots are the leader's point plus a heading-rotated offset (`slot_of`), not points along the route, so in a bending street even a column's trailing slots sit off the street or inside buildings. Fix with H10 (column slots by arc length along the route); re-check again after H3
- [x] Z5 [N6, N2] LOS/viewshed/position-search benchmarks first; then set a budget (entities per side at 4× time compression, incl. projected infantry) and do the LOS speed pass (~50 µs at 4 km vs 20 µs goal) only if the budget is broken. **Benchmarks done 2026-10-21** (`tanks-headless bench`, i5-13500T, release): LOS at 4 km mean 30 µs (`demo`) / 18 µs (`uplands`), p95 74 / 58 µs (the old "~50 µs" matches the p95, not the mean); viewshed 3 km/8 m 64–82 ms; BP search 59–73 ms per call (evaluate, threaded); sim 51× real time with 36 entities (`cw-village-defence`), 190× with 17. Projected 60 entities (≈2.8× the LOS pairs) is ≈18× real time here, 4.6× headroom at 4× compression. **Budget decided 2026-10-21:** reasonable speed and memory on the development machine is enough for now; no LOS speed pass. Optimise (N2) and measure on old hardware when performance becomes a real problem; re-run `tanks-headless bench` after infantry lands to watch the trend
- [ ] Z6 Short design plan `docs/plans/M8B-INFANTRY.md` covering infantry **and** buildings: entity model (which components infantry uses from §4.4), detection signature, weapons, orders, foot mobility, what infantry needs from buildings (attributes, identity, occupancy), whether towns go on a new map or existing ones are re-balanced, replay/hash impact and whether old replays must still load. Review before coding. **Draft written 2026-10-21** (§9 lists 7 decisions with recommendations); **review deferred** by the user, so M8h1a/M8b do not start until it is agreed.
- Exit: all above ticked; plan agreed.

## M8h1a — Buildings for infantry (before infantry, on existing villages)
Only what infantry needs from buildings; full towns wait for M8h1b.
- [ ] H7 Building types with wall thickness/height for LOS and cover values (infantry cover, fields of fire)
- [ ] H8 Building identity in the baked map (ID raster or kept feature list; today only a `building_height` grid): needed by B7, F5, D4
- [ ] H9 Foot mobility class and planning raster (forest, steep slopes, into/through buildings); `MobilityClass` today derives only from `VehicleDef`
- [ ] H10 Narrow streets column-only for vehicles (from D4): formations collapse to column in built-up areas. (Done 2026-10-21, ahead of the rest: slots follow the route by arc length, `ai::formation::route_slot`, so trailing vehicles stay on the street; column through the uplands village 398 → 232 s; 16-seed balance batch neutral)
- Exit: buildings carry type and identity; foot routes through a village; a wedge driving through a village goes to column; baker and hash tests; `BAKER_VERSION` bump.

## M8h1b — Towns (after M8f1)
- [ ] H3 Settlement interiors, town blocks and streets beyond villages — §13.1 M0.5, M6
- Exit: a generated town with streets and enclosed spaces; baker and hash tests; `BAKER_VERSION` bump; Z4 re-checked; balance re-run if existing maps changed.

## M8b-a — Infantry core
- [ ] B1 Dismount/infantry entity class (squads, mount/dismount from IFVs, small arms, posture, low signature) — §2.2, §13.2
- [ ] B3 Coaxial MG — §13.1 M3, M7b
- [ ] B4 HE/HEI vs soft targets (25 mm HEI, HE-FRAG on infantry) — §13.1 M7b
- [ ] B5 Suppression of dismounts (reuse M7d model) — §13.1 M7d
- [ ] B7 Dismounts occupy buildings (uses H8) — supports F5
- [ ] B8 Tall crops matter for infantry concealment — TERRAIN.md §5.3
- [ ] B10 Infantry detection signatures: small target size by posture (standing/prone/dug in/in building), thermal contrast, motion cue; spotters in the picture as small contacts
- [ ] B11 Tank gunnery vs infantry: target class "infantry", ammo selection (HE/canister), coax use, firing rules in the contact drill
- [ ] B14 Minimal DISMOUNT/MOUNT command with hotkey (full plan actions and card in G8), so infantry is playable at once
- [ ] B15 Client display of infantry: NATO infantry symbols in `symbols.rs`, small contacts in the picture, roster and status card, AAR, database entries
- [ ] B16 Doctrine and data: infantry drills in `western-combined` and `generic-opfor` with citations (ATP 3-21.x); squad weapon data per era, sourced
- [ ] B17 BMP-1 class IFV (from I1) so Cold War scenarios have dismounts; duel harness re-run
- [ ] B18 Replay/save versioning for the grown entity model (old replays load or are rejected cleanly, as decided in Z6)
- Exit: IFV platoon dismounts and remounts by player command; tank vs infantry and infantry vs infantry unit tests; infantry detection CSV; golden hash updated deliberately.

## M8b-b — Anti-tank teams
- [ ] B2 ATGM teams on the existing guided-round model — §13.2
- [ ] B6 AI preference for ATGM range — §13.1 M7b
- [ ] B9 Unguided short-range launchers (RPG-7 class): short range, hold-fire-until-close, side/rear-weighted hit chance, back-blast reveals the shooter
- [ ] B12 Close-in vulnerability: tank damage tables for short-range side/rear/top hits (a top-armour value stays M9 T2 for dedicated top-attack)
- [ ] B19 Anti-tank team data (RPG, ATGM per era, sourced) and ambush doctrine in `generic-opfor`
- Exit: a scripted test where an RPG team in a village kills a tank from the flank; per-pairing unit tests; balance batch.

## M8c — Logistics
- [ ] C1 Vehicle ammunition and fuel resupply; supply point or trucks — §11, §13.1 M7b. Plan: `docs/plans/M8C-LOGISTICS.md`. **In progress 2026-10-21**: slices 1–2 done (data `logistics.ron`, scenario `supply_points`, `Command::Resupply` service-station resupply with stock accounting, tests `m8c.rs`); next: re-arm events (C4), client (symbol, card button, status, editor), then tailgate trucks
- [ ] C2 Recovery of immobilised vehicles — §11, §13.2
- [ ] C3 Battery ammunition resupply, reload and relocation — §13.1 M7d
- [ ] C4 Scenario re-arm events
- Note: no hard dependency on infantry (trucks and recovery vehicles are vehicles); placed here by priority and could move earlier.
- Exit: a long scenario where ammunition decides the outcome; stock accounting tests.

## M8d — Engineers and obstacles
- [ ] D1 Obstacles (ditch, wire, traps, abatis) as map features with run-time passage cost — §11, TERRAIN.md §5.7
- [ ] D2 Minefields, mine plough/roller, breaching drill — §13.2
- [ ] D3 Explicit bridges with MLC, fords, demolition; AI treats as chokepoints — TERRAIN.md §5.2, decision #11
- [ ] D4 Building damage → rubble; tanks pushing through light walls (uses H8; narrow streets moved to H10) — TERRAIN.md §5.4, §13.1 M6
- [ ] D6 Editor placement of obstacles and mines
- Exit: breach scenario; planner honours capability; mobility and mine-effect tests.

## M8e — Recon, UAS, communications
- [ ] E1 Cheap UAS reconnaissance feeding the picture (via `PerceivedWorld` only) — §13.2
- [ ] E2 Recon vehicles observe artillery bursts beyond 1.5 km — retired issue 8
- [ ] E3 Electronic and communications effects on reporting — §13.2
- [ ] E4 Code-word acknowledgement and missed transmissions — §13.1 Plans, guide ch. 20
- [ ] E5 Misidentification and fratricide — §13.1 M2, guide glossary (more relevant once dismounts work near tanks; can move up to right after M8f1)
- [ ] E6 Era-dependent own-position lag — §13.1 M2
- [ ] E7 Recon handles contact by calling fire or reporting a bypass — §13.1 Phase 2
- Exit: UAS and degraded-comms scenarios; tests that UAS contacts arrive only via the picture.

## M8f1 / M8f2 — Commander AI and fire-support AI
M8f1 (right after infantry, kept small so infantry is playable): F11, F5, plus B13 as the exit scenario. M8f2 (after M8c–e): F1, F2, F3, F4, F6, F7, F8, F10. F9 is tracked as Z4.
- [ ] F11 Reaction to infantry contacts: tanks use HE/coax and reverse out of close ambush, platoon avoids buildings without dismounted support, IFVs dismount at the right time — new
- [ ] F1 Re-planning and COA choice (HTN/utility), HTN method library — §13.1 M5, M4, Phase 2
- [ ] F2 OPFOR use of plans — §13.1 Plans, Phase 2
- [ ] F3 Blue auto-commander option in the UI — §13.1 M5, Phase 2, content
- [ ] F4 Cross and depth fire patterns — §13.1 M4, Phase 2
- [ ] F5 AI use of buildings — §13.1 M4, M6, Phase 2
- [ ] F6 Route choice around smoke clouds — §13.1 M7a
- [ ] F7 AI adjusting fires, counter-battery, danger-close checks — §13.1 M7d
- [ ] F8 Hide positions, skyline/military-crest scoring — §13.1 M4
- [ ] F10 Fire-on-the-move doctrine differences — §13.1 M3
- [ ] B13 Ambush test scenario: dismounted ATGM/RPG teams in a village vs a tank platoon with and without dismounted support; first infantry scenario (feeds I2). Needs F11, G8 and G9; exit of M8f1/M8g1
- Exit (M8f1): B13 plays with assertions; scripted AI scenario tests with explicit assertions; balance batches across seeds; no hidden-information leaks. Exit (M8f2): same for the remaining items.

## M8g1 / M8g2 — Plans and control UX
M8g1 (with M8f1): G8 and G9. M8g2: G1–G7 after M8f2.
- [ ] G8 Plan actions MOUNT/DISMOUNT and infantry orders in the command card, with hotkeys and tooltips (builds on B14)
- [ ] G9 Scenario editor placement of infantry and teams (part of J5)
- [ ] G1 Phase-line-driven plans on top of the plan model — §13.2
- [ ] G2 Plans for several platoons at once — §13.1 UX pass, Plans
- [ ] G3 Single-vehicle plans — §13.1 Plans
- [ ] G4 Nested condition logic; combine conditions from the menu — §13.1 Plans
- [ ] G5 Plan templates/playbook; execution-matrix screen — §13.1 Plans
- [ ] G6 Separate catch-up speed; night/visibility default speeds — §13.1 Move speed
- [ ] G7 Natural-language order entry as a front-end to existing commands (optional) — §9.5
- Note: `QueueOrder` stays in the sim (AI use, old replays); the client does not issue it.

## M8h2 — Real-world import, generator, seasons
(Urban terrain moved to M8h1a/M8h1b.)
- [ ] H1 Real-world import: DEM (GeoTIFF) + OSM → authoring package; optional GDAL feature — TERRAIN.md §7
- [ ] H2 Generator: stage 4 stamps, stage 7 validation, lakes, hedgerows, rail, tuned micro-relief — §13.1 M0.5, TERRAIN.md §9
- [ ] H4 Baker: tiled/incremental bake, `veg_density`/`canopy_height` rasters; HPA* if large maps need it — §13.1 M0.5, M1
- [ ] H5 Seasons: water level, frozen rivers; season-dependent daylight — TERRAIN.md §5.2, §13.1 M2
- [ ] H6 Performance targets check (12×12 km generate+bake < 60 s on 2013 4-core; incremental re-bake < 1 s; < 500 MB at 16×16 km) — TERRAIN.md §8
- Exit: an imported real map plays a scenario; bake determinism tests; bump `BAKER_VERSION` when baked output changes.

## M8i — Content and scenarios
Run alongside the capability phases: each phase ends with at least one scenario that exercises it.
- [x] I1 Vehicles: T-64/T-80 classes (BMP-1 moved to B17), early-1980s Red and Blue recon, modern recon — §13.1 M7e (data only, sourced, duel harness re-run). **Done 2026-10-24**: `kholm_b` (T-64B/BV, 1976–95), `uval_b` (T-80B/BV, 1978-), `korsak_r2` (BRDM-2), `ward_m3` (M3 CFV, 1983-, M3A3 sensors from 2000), `filin_r3` (BRM-3K, 1995-); rounds `atgm_kobra`, `api_145`. Not wired into any scenario. Limits: no wheeled running gear (the BRDM-2 uses the tracked going model), no radar sensor (BRM-3K), armour not dated (M3A2 appliqué)
- [ ] I2 Scenarios for dismounts, engineers, UAS, logistics once those exist; more on `demo` and `uplands`
- [ ] I3 Mutual-engagement duel mode — §13.1 M7e
- [ ] I4 Campaign ordering and difficulty labels in the scenario list — §13.1 M7e
- [ ] I5 Database: filter by scenario date, side-by-side comparison, silhouettes — §13.1 database
- [ ] I6 National doctrine packs as mods (never code branches) — decision #5
- [x] I8 Re-check `demo` scenario balance (Z1). **Done 2026-10-21**: M8a predates the M7e tuning, so it is not the cause. `mod-fog-meeting` is unchanged since tuning (its 1/7 at seed 3 was already there; the "2–5 lost" note was approximate). `mod-dusk-attack` moved from 19:30 in June to 17:30 on 3 October after the batch (9704297, dusk during the fight): at the old time today's build gives Blue 4/4 wins with 2–5 lost, at the new time 3/4 (seed 2 Red at the time limit, seed 1 with 0 lost). Retuning the dusk scenario is a design call: see I9. 16-seed baseline (`out/b16-*.csv`, not committed): Blue wins 11/16 dusk, 14/16 fog, 15–16/16 the rest
- [ ] I9 Decide whether `mod-dusk-attack` keeps its harder dusk balance (Blue's passive AI lost 1 seed of 4) or is retuned (e.g. earlier start or a smaller Red force); re-run `run --summary` seeds 1–4 after any change

## M8j — Shell, editors, AAR, accessibility
- [ ] J1 Menus: new map from a recipe, new blank scenario, map preview in select screens — §13.1 shell
- [ ] J2 Campaign/scenario progress — §13.1 shell
- [ ] J3 Key rebinding, UI scaling, colour-blind-safe check — §11 accessibility
- [ ] J4 Audio (none yet): radio clicks, squelch, distant fire, volume option — §10.5
- [ ] J5 Scenario editor: weather editor, weather changing mid-scenario, wind, fire support, AI plans and victory conditions, trigger/graphic editing after placement — §13.1 M6, M7a, M7c, M7d
- [ ] J6 AAR: replay by re-simulation (frames are recorded live), graphics in the AAR, PL-relative report text — §13.1 M6
- [ ] J7 Localisation-ready strings — §11

## M8k — Release and platform
- K1 first tag and K3 version string: tracked as Z3.
- [ ] K2 Code signing and notarisation; installer; Intel macOS; Linux AppImage — §13.1 packaging

## ENG — Engine, performance, fidelity (pick up when touched)
- [x] N1 Thermal crossover — §13.1 M2. Already built in M7c (`weather::thermal_factor`, `detection.ron` `thermal_crossover`, test `thermal_contrast_dips_at_crossover`); ticked 2026-10-21
- [ ] N3 Contour/layer `RenderTexture` caching per zoom band — §13.1 M0
- [ ] N4 Internal component model and spall — §13.1 M3
- [ ] N5 Raygui-style panels — §13.1 M0.5
- [ ] N8 Trees detonating HEAT prematurely (moved from D5) — TERRAIN.md §5.3
- N2 (LOS speed) and N6 (benchmarks): tracked as Z5.
- [ ] N7 Offline tuning of utility weights with the headless harness (possible, optional) — §9.5

## M9 — Tier 3 (speculative; data/era packs, never code branches; `estimate` + "speculative" note)
- [ ] T1 Active protection (hard-kill with finite magazine, soft-kill jammers)
- [ ] T2 Top-attack munitions and counters
- [ ] T3 Loitering munitions
- [ ] T4 Directed-energy dazzlers
- [ ] T5 Unmanned ground vehicles as platoon members
- [ ] T6 Multispectral camouflage and signature management
- [ ] T7 Networked fire control (hand-off between platoons)

## V2+ (out of scope until v1.x is done)
- [ ] V1 Campaign layer with persistent crews and vehicle damage
- [ ] V2 Multiplayer lockstep (sim is already deterministic)
- [ ] V3 Aviation (power lines/pylons then matter) — TERRAIN.md §5.7
- [ ] V4 Optional menu music

## Done
- 2026-10-21: all nine items of the retired `KNOWN-ISSUES.md` (seed mismatch, class counts, database range/ERA, dead queue UI, AAR kill credit, replay path, duplicate name, ADJUST reason, M8a doc drift), except its recon-range suggestion (now E2).
- Milestones M0–M7e, M8a (queue, reverse, going): see `DESIGN.md` §13.1.
