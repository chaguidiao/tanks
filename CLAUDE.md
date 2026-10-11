# CLAUDE.md — Tanks

**Tanks** (working title): top-down, real-time-with-pause **tank command simulation**. The player commands tank platoons with intent-level orders; vehicle/platoon AI executes them using real doctrine. Serious tone, tactical-display aesthetics, physically grounded models.

- Full design: `docs/DESIGN.md` (read before any non-trivial change); decisions log in §14
- Terrain, map format, generation, editor: `docs/TERRAIN.md`
- Sources: `docs/REFERENCES.md`
- Dev environment (container, commands): `docs/DEVELOPMENT.md`
- Status: **pre-implementation**. Stack: **Rust sim core + raylib 6.x client (`raylib-rs`)**. Licence MIT. Next: milestone M0 (DESIGN.md §13).

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

## Visual style
Dark, low-saturation tactical display; monospace uppercase labels; NATO symbology (MIL-STD-2525D / APP-6D); colour is never the only cue. Avoid flashy effects, bright gradients, or particle spam.

## Dev environment
- All builds, tests and tools run **inside the `tanks-dev` container** (see `docs/DEVELOPMENT.md`); don't install toolchains or packages on the host.
- From the host: `tools/dev/devbox.sh run <cmd>` (e.g. `tools/dev/devbox.sh run cargo nextest run`). Headless rendering checks use `xvfb-run` inside the container.

## Workflow
- Licence: MIT (`LICENSE`). List any bundled third-party asset and its licence in `THIRD_PARTY.md`. Only bundle assets/data with compatible licences; write vehicle data in our own words and tables, citing sources rather than copying them.
- Small, reviewable commits per subsystem. Update `docs/DESIGN.md` when a design decision changes.
- When unsure about a doctrinal or physical detail, consult `docs/REFERENCES.md` and say what the source says rather than guessing.
