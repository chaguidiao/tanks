# CLAUDE.md — Tanks

**Tanks** (working title): top-down, real-time-with-pause **tank command simulation**. The player commands tank platoons with intent-level orders; vehicle/platoon AI executes them using real doctrine. Serious tone, tactical-display aesthetics, physically grounded models.

- Full design: `docs/DESIGN.md` (read before any non-trivial change); decisions log in §14
- Terrain, map format, generation, editor: `docs/TERRAIN.md`
- Sources: `docs/REFERENCES.md`
- Dev environment (container, commands): `docs/DEVELOPMENT.md`
- Status: **version 0.1.2** (2026-10-31). Done: M0–M7 (v1.0: tanks, terrain, AI, plans, smoke, weather, fire support, content, packaging), M8a, M8h1a/b (buildings, towns), M8b (infantry, anti-tank teams), M8f1/M8g1 (infantry AI, plans and editor), M8c slices 1–6 (supply points, LOGPACs, battery resupply, tailgate supply trucks). **Next: M8c slice 7 (C2 recovery)**, `docs/plans/M8C-LOGISTICS.md`. Stack: **Rust sim core + raylib 6.x client (`raylib-rs`)**, licence MIT. Per-phase detail: DESIGN.md §13.1 and §14; open work and order: `docs/TODO.md`; release rule and checklist: `docs/DEVELOPMENT.md` Releases (TODO K4).

## Architecture rules (non-negotiable)
1. **`sim` is graphics-agnostic.** No raylib/rendering/UI types in `sim`, `terrain`, `terragen`, `ai`, `sim-data`. All raylib calls live in `client` behind a small `render` module. The client talks to the sim only via `Command` in, `Event` out, and `PerceivedWorld` reads.
2. **Determinism.** Fixed 20 Hz tick. Same seed + same command log ⇒ same state hash.
   - No wall-clock time, no unseeded RNG, no `HashMap`/`HashSet` iteration in sim logic (use `BTreeMap`, `IndexMap`, or sorted `Vec`).
   - Parallel work (rayon) only in read-only phases; merge results in a stable order.
   - One seeded `ChaCha` stream per subsystem.
3. **Fog of war is enforced by types.** Player UI and every side's AI read `PerceivedWorld` only. `TruthWorld` access is for AAR, tests, and the `debug-truth` feature.
4. **Time compression = more ticks**, never a larger `dt`.
5. **Not everything is a tank.** Use composable components so infantry/artillery can be added without rewrites.
6. **Era is data.** No `if era == ...` in code. Vehicles, subsystems and capabilities carry in-service date ranges; a scenario date plus per-side overrides decide what exists.
7. **Old hardware is a target.** The client must run on OpenGL 3.3 and on the OpenGL 2.1 build (~2013 GPUs). Every shader ships GLSL 330 + GLSL 100 variants. No compute shaders; heavy work goes to the CPU.
8. **Author → bake → run for terrain.** The authoring package (GeoTIFF + GeoJSON) is the source of truth; the baked package is derived, deterministic and never hand-edited. Generator, importer and editor all write the authoring format.

## Data rules
- All vehicle/ammo/doctrine numbers live in `data/**/*.ron`, never hard-coded.
- Every numeric stat carries `source`, `confidence` (`high|medium|low|estimate`) and optional `notes`. If no public source exists, mark `estimate` and explain the reasoning.
- Vehicles are **fictional names** mirroring real designs. Public sources only — never seek or use restricted/classified data.
- Units: SI internally (m, s, kg, rad, m/s). Display layer converts to mils, km/h, MGRS-style grids.

## Doctrine rules
- Every AI behaviour maps to a doctrinal task/drill and cites it in a doc comment (e.g. `// ATP 3-20.15 App. F: React to direct fire contact`).
- Doctrine lives in packs (`data/doctrine/<pack>/`); a scenario assigns a pack per side. Baselines: `western-combined`, `generic-opfor`. National doctrines (e.g. Soviet/Russian) are mod packs — never code branches.
- Radio/report text uses terse, realistic brevity (callsigns, grid, direction, distance) — no jokes, no flavour chatter.

## Code conventions
- Cargo workspace; `crates/{sim,sim-data,terrain,terragen,ai,client,headless}`.
- `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo nextest run` must pass before commit.
- Pin `raylib`/`raylib-rs` versions; upgrades are their own commits.
- Keep dependencies minimal; justify each new crate in the commit message.
- Physics/ballistics formulas: unit-test with reference values and cite the source in the test.
- Prefer clear, plain data structures over clever generics. Keep the client thin.

## Testing
- Unit + property tests for formulas and LOS.
- Golden replay hash tests for determinism (in CI).
- AI scenario tests on small synthetic maps with explicit assertions (e.g., "all four tanks hull-down vs. threat azimuth within 120 s").
- Validation harness in `headless` outputs CSV (Ph/Pk vs. range, time-to-detect); don't change tuning without re-running it.

## Codebase map (where things live)
- `sim-data`: `Stat<T>` (value/source/confidence), `terrain` (land cover, soils, building types, foot mobility), `smoke` (smoke kinds, launchers), `fires` (batteries, HE effects, suppression), vehicle (sensors, FCS, armament, armour)/ammo/combat/doctrine/terrain/scenario defs, `DataSet::load`, `load_scenario`/`save_scenario`. RON files use the `IMPLICIT_SOME` extension (see `ron_options()`).
- `terrain`: `Grid`, `Heightfield` (row 0 = south, node at cell centre), `noise`, `micro::h_micro`, `contour`, `features` (GeoJSON), `geotiff`, `package::AuthoringPackage`, `bake::{bake, BakedMap, BakedBuilding}` (incl. building id raster and entrances), `ground::Ground` (height incl. micro-relief, canopy, buildings), `los` (sight, viewshed), `mobility` (terrain caps, planning speed raster, vehicle and `foot` classes), `path::Pathfinder` (A* + string pulling), `shade` (CPU images incl. `tactical`), `png`, `hash::StableHasher`.
- `terragen`: stages (`landform`, `hydro`, `rivers`, `cover`, `roads`, `town` (market town: zoned street lattice, blocks, streets)), `generate()`, `workflow::ensure_map` (load-or-generate-and-bake). Recipes in `data/recipes/`.
- `ai`: pure functions with plain inputs (no `sim` types). `picture.rs` = `PerceivedWorld` (the only thing AI/UI read; also the plan data types `Plan`/`PlanStep`/`LegSettings`/`StepAction`/`ConditionSet`/`Branch`/`ContactReaction`), `drive.rs` route following, `formation.rs`, `position.rs` (BP hull-down finder), `firedist.rs`, `commander.rs` (attack with assault position, defend with alternates and displacement, reserve, recon; `tests/phase2.rs`).
- `sim`: `Sim` (`submit`/`submit_immediate`/`settle`/`step`/`perceived`/`truth`/`state_hash`, `Replay`), `world.rs` (SoA entities, `Order`), `vehicle.rs` (vehicle AI 5 Hz), `mobility.rs` (physics), `sensors.rs` (detection 4 Hz), `smoke.rs` (cloud field, launchers, auto-screen), `infantry.rs` (squads: foot movement, posture, react to contact, dismount/mount drills, dismount platoons, occupancy, building choice, shoot and move, signatures/facades, losses with the carrier), `builtup.rs` (route rasters round built-up ground), `smallarms.rs` (burst model, coax, HE vs infantry), `antiarmour.rs` (squad rockets and missiles: set-up, ambush, operator in the loop), `weather.rs` (sun elevation, light/thermal factors, atmosphere), `fires.rs` (batteries, call-for-fire missions, impacts, HE effects, suppression), `picture.rs` (per-side picture, contacts), `gunnery.rs` (FCS, ballistics, per-round-type penetration, CE/ERA, `select_round`, smoke missions, damage), `platoon.rs`/`platoon_ai.rs` (1 Hz), `plan.rs` (checkpoint plan runner, 1 Hz, through the existing orders), `control.rs` (graphics, triggers, condition checks), `victory.rs`, `command.rs` (`Command`, `Event`). Scenario tests: `crates/sim/tests/m1..m6.rs`, `m7a.rs` (smoke), `m7b.rs` (round types, ERA, ammo selection), `m7c.rs` (weather, night, fog, dust), `m7d.rs` (call for fire, dispersion, suppression, drill), `plans.rs` (plans, waits, code words, branches, contact reaction), `m8a.rs` (order queue, reverse driving, wet going), `m8b.rs` (infantry), `m8b_at.rs` (anti-tank teams), `m8g.rs` (infantry plans), `m8f.rs` (ambush drill, built-up routes, commander dismounts, buildings); `crates/headless/tests/b13.rs` (B13, ignored by default), `m8h.rs` (routes, formations and squads on the `town` map); `crates/terragen/tests/town.rs` (town generator).
- `client`: `main.rs` (`App`: screen state machine title → main menu → select → loading thread → map screen, overlays, CLI scripting), `menu.rs` (title, main menu, scenario/map select + briefing, loading, options, Esc menu, confirm; contour backdrop), `database.rs` (DATABASE screen: data sheets with source/confidence, CALC values from sim formulas), `mapscreen.rs` (`MapScreen` = game / terrain editor / scenario editor screen: own top bar, help, save/bake/play test), `settings.rs` (options file), `render.rs` is the ONLY raylib module (incl. the UI font, `data/fonts/`, fullscreen/fps); `view.rs` (camera, map layers, minimap), `hud.rs` (buttons incl. menu look + keyboard focus, tooltips, screen layout), `game.rs` (sim loop, planning, orders/routes, command card, roster, panels, radio text) with `game/ctx.rs` (right click, context menus), `game/planedit.rs` (plan draft, CP/leg menus, plan drawing, ETA, step list), `game/posture.rs` (FORMATION/TECHNIQUE/SPEED/WEAPONS/ON CONTACT/AMMO value lists, leg or now scope; GRAPHICS list), `game/feedback.rs` (ghost order line, draft strip, hint line) and `game/codewords.rs` (code-word strip, readiness, auto-pause), `game/gfxedit.rs` (move/resize/turn graphics, contingency rows and menu after placement), `hud.rs` also has the `PopupMenu` widget, `aar.rs` (recorder + after-action review), `editor.rs` (terrain, background `BakeQueue`), `scenedit.rs` (scenario editor: panel pages UNITS / TIME-WX / FIRES / VICTORY as value rows, supply points), `symbols.rs`, `format.rs` (grids/mils/km/h), `style.rs` (palette).
- `headless`: `tanks-headless gen|bake|render|info|probe|run|duel|detect|firefight|replay` (`run --seed n --summary` for balance batches, `duel --year y` (`--shooter <squad>` for launchers); `firefight` for small-arms rounds per casualty; `tests/scenarios.rs` plays the v1.0 scenarios).
- Status, deferred items and the next milestone: DESIGN.md §13.1. Decisions taken during implementation: DESIGN.md §14 (#10–15).

## Verifying changes
- `tools/dev/devbox.sh run just check` (fmt + clippy + nextest). Also clippy the client with `--no-default-features --features gl21,debug-truth`.
- Visual check without a display: `xvfb-run -a -s "-screen 0 1600x900x24" target/debug/client --order 1-1:3000,2600 --advance 60 --zoom 1.2 --center 2000,1900 [--layer traffic|slope|cover] [--edit] --screenshot out/x.png`, then look at the PNG. Menus: `--screen title|main|play|edit-scenario|edit-terrain|database|options`, `--keys enter,wait30,down,esc` (one key per frame; loads don't count), `--pause` (Esc menu over a direct screen). M6 options: `--scenario m5-hold-the-village`, `--order 1:cpx,cpy,..,x,y` (route with checkpoints), `--pl -:x,y,x,y`, `--bp 1:x,y,deg`, `--select 1 --press F,W,T,H,S` (command-card keys, applied before `--advance`; `--select 1-2` selects one vehicle: status card), `--aar`, `--scenario-editor`, `--save-replay out/r.ron` (check with `tanks-headless replay --file out/r.ron`; it prints plan reports and code words). Plans: `--plan 1:x,y,x,y` or `--plan 1:@docs/plans/example-plan.ron` (sent; add `--plan-draft` to leave it as a draft for menu screenshots), `--advance 300 --code-word HAMMER --advance 400` (run in order), `--rclick x,y[,hold_s]` (screen position; held = context menu; add `--shift` to hold Shift: appends a checkpoint) then `--keys c,right,..` (C = ON CONTACT list). `--click x,y` = scripted left click (also puts the pointer there; editor keys act at the pointer, add `--frames n` so they run). Map rasters: `tanks-headless render --map demo --layer tint|landcover|traffic --downsample 3 --out out/x.png`.
- The golden replay hash in `crates/sim/tests/m1.rs` breaks on any behaviour change: update it deliberately and say why in the commit.

## Gotchas
- `data/maps/` is gitignored; maps regenerate from recipe + seed. Changing generator/baker output changes the demo map; bump `bake::BAKER_VERSION` when baked output changes for the same input.
- Baked elevation is quantised (2 cm) in memory too, so fresh and loaded bakes are bit-identical; keep it that way.
- `serde_json` needs `float_roundtrip` for exact GeoJSON round-trips (already enabled).
- `gen` is a reserved keyword in edition 2024.
- Use a separate `CARGO_TARGET_DIR=target/gl21` for the GL 2.1 build to avoid rebuilding raylib.
- Podman under WSL2 works for `devbox.sh run`.
- `devbox.sh run bash -c 'xvfb-run ...'` can hang (xvfb-run as PID 1 misses Xvfb's ready signal): end the script with `; true`.
- Clicks: every button is built by `Game::buttons`/`MapScreen::buttons`/`menu::*_buttons` (same layout for hit-testing and drawing); hotkeys map to the same actions.
- Any game/editor CLI option skips the menus (`Opts::direct`); without them the client starts at the title screen.

## Visual style
Dark, low-saturation tactical display; monospace uppercase labels; NATO symbology (MIL-STD-2525D / APP-6D); colour is never the only cue. Avoid flashy effects, bright gradients, or particle spam.

## Dev environment
- All builds, tests and tools run **inside the `tanks-dev` container** (see `docs/DEVELOPMENT.md`); don't install toolchains or packages on the host.
- From the host: `tools/dev/devbox.sh run <cmd>` (e.g. `tools/dev/devbox.sh run cargo nextest run`). Headless rendering checks use `xvfb-run` inside the container.

## Workflow
- Licence: MIT (`LICENSE`). List any bundled third-party asset and its licence in `THIRD_PARTY.md`. Only bundle assets/data with compatible licences; write vehicle data in our own words and tables, citing sources rather than copying them.
- Small, reviewable commits per subsystem. Update `docs/DESIGN.md` when a design decision changes.
- Open work is tracked in `docs/TODO.md` (master list). Add new deferrals there, tick items when done, and keep the "Deferred" notes in DESIGN.md §13.1 as history only.
- When unsure about a doctrinal or physical detail, consult `docs/REFERENCES.md` and say what the source says rather than guessing.

## Subagents (model choice)
Always set `model` explicitly when starting a subagent; never leave it to inherit the session model.
- **opus**: design-heavy or sim-changing work (new sim behaviour, determinism/hash impact, AI and doctrine), and judgement-heavy data work (sourcing vehicle/ammo stats, confidence levels, balance or tuning calls).
- **sonnet** (default): implementation against a clear spec or plan, client/UI and editor work, tests, docs updates.
- **haiku**: quick mechanical jobs only (codebase searches, log summaries, bulk renames); never for work where the architecture, data or doctrine rules above apply.
- Parallel agents work in their own git worktrees, commit only to their branch and never push; the session reviews and merges into `main` one branch at a time (sim branches conflict on the golden hash in `crates/sim/tests/m1.rs`).
