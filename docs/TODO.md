# Master to-do list

The single place to track what is not done. Last consolidated 2026-10-21 from: `DESIGN.md` §2.2, §9.5, §11, §13, §13.1 (Deferred lists), §13.2; `TERRAIN.md` §5, §7–9; the archived plans; the user guide's stated limits; the retired `KNOWN-ISSUES.md`. No `TODO`/`FIXME` markers exist in code.

How to use: items have an id (phase letter + number). Tick `[x]` when done and say so in the commit; when a phase ends, update `DESIGN.md` §13.1 and move finished items to the "Done" note at the bottom. Add new deferrals here first, not into scattered "Deferred" lines. Nothing below is scheduled until the order is agreed; **bold** = suggested next.

Rules for every item (CLAUDE.md): data in `data/**/*.ron` with source and confidence, no `if era`, sim stays graphics-agnostic, golden hash changes only deliberately, balance batches/duel harness re-run when tuning moves, docs updated.

## Phase map (v1.x = M8, v1.5 = M9, then v2+)

| Phase | Theme | Depends on |
|---|---|---|
| M8b | Dismounts and ATGM teams (first non-tank classes) | none |
| M8c | Logistics: resupply, recovery | M8b optional |
| M8d | Engineers, obstacles, minefields, bridges, destruction | M8b |
| M8e | Recon, UAS, communications, identification | M8b |
| M8f | Commander AI and fire-support AI | M8b–d |
| M8g | Plans and control UX | best after M8f |
| M8h | Terrain: real-world import, generator, urban, seasons | independent |
| M8i | Content and scenarios | M8b–e for new-class scenarios |
| M8j | Shell, editors, AAR, accessibility | independent |
| M8k | Release and platform | independent (first tag can be any time) |
| ENG | Engine, performance, fidelity backlog | as touched |
| M9 | Tier 3 speculative systems | M8 |
| V2+ | Campaign, multiplayer, aviation, optional extras | later |

## M8b — Dismounts and ATGM teams
- [ ] B1 Dismount/infantry entity class (squads, mount/dismount from IFVs, small arms, posture, low signature) — §2.2, §13.2
- [ ] B2 ATGM teams on the existing guided-round model — §13.2
- [ ] B3 Coaxial MG — §13.1 M3, M7b
- [ ] B4 HE/HEI vs soft targets (25 mm HEI, HE-FRAG on infantry) — §13.1 M7b
- [ ] B5 Suppression of dismounts (reuse M7d model) — §13.1 M7d
- [ ] B6 AI preference for ATGM range — §13.1 M7b
- [ ] B7 Dismounts occupy buildings — new, supports F5
- [ ] B8 Tall crops matter for infantry concealment — TERRAIN.md §5.3
- Exit: IFV platoon dismounts, ATGM team ambushes a column; hash stable; per-pairing unit tests; infantry detection CSV.

## M8c — Logistics
- [ ] C1 Vehicle ammunition and fuel resupply; supply point or trucks — §11, §13.1 M7b
- [ ] C2 Recovery of immobilised vehicles — §11, §13.2
- [ ] C3 Battery ammunition resupply, reload and relocation — §13.1 M7d
- [ ] C4 Scenario re-arm events
- Exit: a long scenario where ammunition decides the outcome; stock accounting tests.

## M8d — Engineers and obstacles
- [ ] D1 Obstacles (ditch, wire, traps, abatis) as map features with run-time passage cost — §11, TERRAIN.md §5.7
- [ ] D2 Minefields, mine plough/roller, breaching drill — §13.2
- [ ] D3 Explicit bridges with MLC, fords, demolition; AI treats as chokepoints — TERRAIN.md §5.2, decision #11
- [ ] D4 Building damage → rubble; tanks pushing through light walls; narrow streets column-only — TERRAIN.md §5.4, §13.1 M6
- [ ] D5 Trees detonating HEAT prematurely — TERRAIN.md §5.3
- [ ] D6 Editor placement of obstacles and mines
- Exit: breach scenario; planner honours capability; mobility and mine-effect tests.

## M8e — Recon, UAS, communications
- [ ] E1 Cheap UAS reconnaissance feeding the picture (via `PerceivedWorld` only) — §13.2
- [ ] E2 Recon vehicles observe artillery bursts beyond 1.5 km — retired issue 8
- [ ] E3 Electronic and communications effects on reporting — §13.2
- [ ] E4 Code-word acknowledgement and missed transmissions — §13.1 Plans, guide ch. 20
- [ ] E5 Misidentification and fratricide — §13.1 M2, guide glossary
- [ ] E6 Era-dependent own-position lag — §13.1 M2
- [ ] E7 Recon handles contact by calling fire or reporting a bypass — §13.1 Phase 2
- Exit: UAS and degraded-comms scenarios; tests that UAS contacts arrive only via the picture.

## M8f — Commander AI and fire-support AI
- [ ] F1 Re-planning and COA choice (HTN/utility), HTN method library — §13.1 M5, M4, Phase 2
- [ ] F2 OPFOR use of plans — §13.1 Plans, Phase 2
- [ ] F3 Blue auto-commander option in the UI — §13.1 M5, Phase 2, content
- [ ] F4 Cross and depth fire patterns — §13.1 M4, Phase 2
- [ ] F5 AI use of buildings — §13.1 M4, M6, Phase 2
- [ ] F6 Route choice around smoke clouds — §13.1 M7a
- [ ] F7 AI adjusting fires, counter-battery, danger-close checks — §13.1 M7d
- [ ] F8 Hide positions, skyline/military-crest scoring — §13.1 M4
- [ ] F9 **Re-check**: wedge routed through a village spends minutes re-planning trailing slots (`platoon_ai::move_step`, re-routes each slot every 4 s or 25 m); not reproduced yet — §13.1 Plans "Open"
- [ ] F10 Fire-on-the-move doctrine differences — §13.1 M3
- Exit: scripted AI scenario tests with explicit assertions; balance batches across seeds; no hidden-information leaks.

## M8g — Plans and control UX
- [ ] G1 Phase-line-driven plans on top of the plan model — §13.2
- [ ] G2 Plans for several platoons at once — §13.1 UX pass, Plans
- [ ] G3 Single-vehicle plans — §13.1 Plans
- [ ] G4 Nested condition logic; combine conditions from the menu — §13.1 Plans
- [ ] G5 Plan templates/playbook; execution-matrix screen — §13.1 Plans
- [ ] G6 Separate catch-up speed; night/visibility default speeds — §13.1 Move speed
- [ ] G7 Natural-language order entry as a front-end to existing commands (optional) — §9.5
- Note: `QueueOrder` stays in the sim (AI use, old replays); the client does not issue it.

## M8h — Terrain
- [ ] H1 Real-world import: DEM (GeoTIFF) + OSM → authoring package; optional GDAL feature — TERRAIN.md §7
- [ ] H2 Generator: stage 4 stamps, stage 7 validation, lakes, hedgerows, rail, tuned micro-relief — §13.1 M0.5, TERRAIN.md §9
- [ ] H3 Settlement interiors, town blocks and streets beyond villages — §13.1 M0.5, M6
- [ ] H4 Baker: tiled/incremental bake, `veg_density`/`canopy_height` rasters; HPA* if large maps need it — §13.1 M0.5, M1
- [ ] H5 Seasons: water level, frozen rivers; season-dependent daylight — TERRAIN.md §5.2, §13.1 M2
- [ ] H6 Performance targets check (12×12 km generate+bake < 60 s on 2013 4-core; incremental re-bake < 1 s; < 500 MB at 16×16 km) — TERRAIN.md §8
- Exit: an imported real map plays a scenario; bake determinism tests; bump `BAKER_VERSION` when baked output changes.

## M8i — Content and scenarios
- [ ] I1 Vehicles: BMP-1, T-64/T-80 classes, early-1980s Red and Blue recon, modern recon — §13.1 M7e (data only, sourced, duel harness re-run)
- [ ] I2 Scenarios for dismounts, engineers, UAS, logistics once those exist; more on `demo` and `uplands`
- [ ] I3 Mutual-engagement duel mode — §13.1 M7e
- [ ] I4 Campaign ordering and difficulty labels in the scenario list — §13.1 M7e
- [ ] I5 Database: filter by scenario date, side-by-side comparison, silhouettes — §13.1 database
- [ ] I6 National doctrine packs as mods (never code branches) — decision #5
- [ ] I7 Balance re-run on seed-7 uplands after the seed fix (scenarios were tuned there; unverified since 2026-10-21 change)

## M8j — Shell, editors, AAR, accessibility
- [ ] J1 Menus: new map from a recipe, new blank scenario, map preview in select screens — §13.1 shell
- [ ] J2 Campaign/scenario progress — §13.1 shell
- [ ] J3 Key rebinding, UI scaling, colour-blind-safe check — §11 accessibility
- [ ] J4 Audio (none yet): radio clicks, squelch, distant fire, volume option — §10.5
- [ ] J5 Scenario editor: weather editor, weather changing mid-scenario, wind, fire support, AI plans and victory conditions, trigger/graphic editing after placement — §13.1 M6, M7a, M7c, M7d
- [ ] J6 AAR: replay by re-simulation (frames are recorded live), graphics in the AAR, PL-relative report text — §13.1 M6
- [ ] J7 Localisation-ready strings — §11
- [ ] J8 Screenshot check of the screens changed on 2026-10-21 (scenario select, database, ADJUST tooltip) — not yet viewed

## M8k — Release and platform
- [ ] K1 **Cut the first tag** to run the untested Windows and macOS legs of `release.yml` — §13.1 packaging
- [ ] K2 Code signing and notarisation; installer; Intel macOS; Linux AppImage — §13.1 packaging
- [ ] K3 Version string on the title screen — §13.1 packaging

## ENG — Engine, performance, fidelity (pick up when touched)
- [ ] N1 Thermal crossover — §13.1 M2
- [ ] N2 LOS speed target (~50 µs at 4 km vs 20 µs goal) — §13.1 M2
- [ ] N3 Contour/layer `RenderTexture` caching per zoom band — §13.1 M0
- [ ] N4 Internal component model and spall — §13.1 M3
- [ ] N5 Raygui-style panels — §13.1 M0.5
- [ ] N6 Benchmarks for LOS/viewshed/position search — §12
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
