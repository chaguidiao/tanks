# Design Document — Tanks (working title)

Status: **Accepted baseline (2026-10-11)**. M0–M6 are implemented (lean versions, §13.1). Changes to decisions go in §14.

---

## 1. Vision

A top-down, real-time-with-pause tank command simulation. The player is a **company/platoon commander looking at a tactical display**, not a driver and not an RTS god. The player issues *intent-level orders* ("occupy hull-down positions in this area, sector of fire from here to here, engage at 2,500 m") and the vehicles' built-in AI works out exact positions, routes, targets and timing using real doctrine.

Touchstones:

| Reference | What we take from it |
|---|---|
| ProSIM *ATF / BCT Commander* | Seriousness, reported-picture fog of war, order delay, phase-based planning, no hand-holding |
| MicroProse *M1 Tank Platoon I/II* (commander/map mode) | Scale (1–4 platoons), personal attachment to crews, campaign of engagements |
| *Armored Brigade I/II* (Matrix) | Closest modern relative: pausable real-time, real-world terrain, Cold War doctrine; study its UI and LOS tooling |
| *Steel Beasts Pro PE* | Gunnery, FCS and damage fidelity; map-mode orders and "tactical AI follows routes with behaviours" |
| *Combat Mission* series | TacAI that reacts to contact on its own; LOS/LOF tool |
| *Graviteam Tactics* | Orders with delay, crews acting on their own under pressure |
| ATP 3-20.15 *Tank Platoon* (2025) | The rulebook for what orders exist and how the AI executes them |

**Design pillars**
1. **Command, don't control.** Orders express intent; subordinates execute. Micro-control is possible but costs time and attention, exactly as on a real net.
2. **You only see what is reported.** The display is the *perceived* picture assembled from crew spot reports — late, partial and sometimes wrong. Ground truth is shown only in the after-action review.
3. **Terrain is the main character.** Continuous elevation, micro-relief, vegetation and soil decide who sees whom first, and that decides who wins.
4. **Physics and doctrine are authentic.** Every vehicle number has a cited public source. Every AI behaviour maps to a doctrinal task.
5. **Quiet, serious presentation.** A 1990s-film-style tactical display: dark, monochrome-leaning, vector lines, NATO symbology, terse radio text.

---

## 2. Scope

### 2.1 In scope for v1.0
- Single player vs. AI, offline.
- Player commands **1 tank company team (up to 3 platoons × 4 tanks + 2-tank HQ section = 14 tanks)**. Default scenario size: 1–2 platoons. Enemy force of similar or larger size (OPFOR tank companies/battalion elements).
- Unit type: **main battle tanks only** (fictional designs modelled on real vehicles; see §2.3 for era coverage).
- Maps: ~8 × 8 km to 15 × 15 km, **procedurally generated** and hand-edited in the built-in terrain editor (real-world import later; same format). See `docs/TERRAIN.md`.
- Real time with pause, time compression (×1, ×2, ×4, ×8 when not in contact).
- Pre-battle planning phase (graphics/control measures, task organisation, phased orders).
- Fog of war via sensor + detection model + reporting chain.
- Ballistics, hit probability, armour penetration, component damage.
- Platoon and vehicle AI executing doctrinal tasks and battle drills.
- Scenario format + a small set of scenarios; after-action review (replay with ground truth).

### 2.2 Explicitly out of scope for v1.0 (designed for, not built)
- Infantry, IFVs, ATGM teams, artillery, aviation, UAS/drones, engineers, logistics vehicles.
  *The sim core must not assume "everything is a tank" — use component-based entities so these can be added.*
- Multiplayer (keep the sim deterministic so lockstep is possible later).
- Campaign layer (M1TP-style persistent crews) — planned for after v1.0.
- 3D view of any kind.

### 2.3 Era / setting — **decided: Cold War → modern → near future**
The setting is **fictional countries** with a scenario **date**. The date decides which technology and doctrine is available. Era is *data*, not code: every vehicle, subsystem and capability has an in-service date range, and every scenario picks a date plus per-side tech overrides.

Era drives far more than the vehicle list. The biggest gameplay differences are in sensing and C2:

| Capability | Cold War (~1975–1991) | Modern (~1992–2020) | Near future (2020s+) |
|---|---|---|---|
| Gunner's sight | Day optics, image intensifier, first-gen thermals (West ~1980+) | 2nd-gen thermals for both sides | 3rd-gen, multispectral, sensor fusion |
| Commander's sight | Periscopes, unbuttoned eyes | Independent thermal viewer (hunter-killer) | 360° camera ring, automated target cueing |
| Fire control | Laser rangefinder + analogue/early digital computer; fire-on-the-move limited | Full digital FCS, good fire-on-the-move | Auto-tracking, programmable rounds |
| Own-force picture (C2) | Voice radio + paper map: **the player's own positions also lag and drift** | Digital battle management / blue-force tracking: near-live own positions, shared contacts | Networked shared picture; EW can degrade it |
| Protection | Steel + early composite, first ERA | Composite + ERA, early APS | APS widespread, signature management |
| Threats beyond tanks (out of v1 scope) | ATGMs, artillery, helicopters | + top-attack ATGMs | + UAS, loitering munitions, FPV drones, EW |

**v1 content plan:** the engine is era-agnostic from day 1. v1 ships two content sets: **Cold War (~1985)** and **Modern (~2005)**. Near-future content is gated on UAS/C-UAS and EW modelling (v2), because tanks-only play in a drone-saturated setting isn't credible.

**Doctrine:** Blue baseline from ATP 3-20.15 (2025) plus FM 17-15 (Cold War). Red is a **generic, data-driven OPFOR doctrine** (§9.4). A Soviet/Russian-style doctrine pack can be added as a mod using the same schema (TC 7-100.x, FM 100-2-1, Grau).

---

## 3. Technology stack

### 3.1 Requirements that drive the choice
- Cross-platform desktop (Windows, Linux, macOS). **Must run on ~2013-era GPUs** (e.g., Intel HD 4000, GeForce GTX 600 series, Radeon HD 7000 series).
- Minimal, established, low-churn dependencies.
- CPU-heavy simulation (LOS, viewsheds, position search, pathfinding) → native performance, multithreading.
- Deterministic, headless-runnable simulation for testing, Monte-Carlo validation and replays.
- 2D vector rendering with thousands of contour polylines, plus a shaded raster underlay.
- Data-heavy UI (unit panels, order dialogs, logs, timelines) plus a terrain editor.

### 3.2 Decision: Rust sim core + **raylib** client (via `raylib-rs`)

**raylib** (zlib licence, C99, 2013–present) is small, stable and established. **raylib 6.0** (April 2026) can be built with any of these graphics backends:

| Backend | Hardware it covers |
|---|---|
| OpenGL 3.3 (default) | Essentially every discrete GPU since ~2010; Intel HD 4000+ (on Windows, HD 2000/3000 only reach GL 3.1 → use the 2.1 build) |
| OpenGL 2.1 | Anything from the mid-2000s onward |
| OpenGL ES 2.0 | Raspberry Pi / ARM SBCs, WebGL via Emscripten |
| Software renderer (`rlsw`, new in 6.0) | No GPU at all; slower but usable for a 2D display |

So **yes, raylib runs on a 2013 card**, with fallbacks below that. **SDL is already covered**: raylib can use SDL as its platform/windowing backend, so we don't need a separate SDL fallback. (Plain SDL3's new GPU API needs Vulkan/D3D12/Metal, which many 2013 GPUs don't have — another reason raylib's OpenGL path is the safer choice.) On macOS, OpenGL is deprecated but still works (GL 4.1 core, translated on Apple Silicon).

```
tanks/                     (Cargo workspace)
├── crates/
│   ├── sim/        # pure Rust, no graphics deps. Deterministic. THE game.
│   ├── sim-data/   # serde types + loaders for vehicles, ammo, doctrine, eras, scenarios (RON/TOML)
│   ├── terrain/    # map format, heightfield, layers, LOS/viewshed, contours, cost grids
│   ├── terragen/   # procedural generation pipeline + importers (DEM/OSM later). No graphics deps.
│   ├── ai/         # platoon/vehicle/commander AI
│   ├── client/     # raylib app: game + terrain editor modes. Talks to sim via Commands/Events/PerceivedWorld
│   └── headless/   # CLI: generate/bake maps, batch runs, validation harness, replay verifier, benchmarks
├── data/           # vehicles/, ammo/, doctrine/, eras/, scenarios/, maps/
└── docs/
```

Why Rust + raylib:
- **raylib** gives a window, input, OpenGL batching, textures, render targets, shaders, TTF fonts, audio and `raygui`. Nothing else. That matches "bare minimal": no engine framework, no scene system, no editor to fight.
- **Rust** for the core: C/C++-class performance, memory safety, `rayon` for parallel LOS, strong testing, and the compiler catches many mistakes in AI-assisted development.
- **`raylib-rs` 6.0.0** tracks raylib 6.0. It is a community binding: we keep the client thin and isolate raylib calls behind a small `render` module. If the binding ever lags, `raylib-ffi`/raw `bindgen` is the escape hatch.

Client-side:

| Need | Choice |
|---|---|
| Window / input / renderer / audio | `raylib` 6.x via `raylib-rs` (build features choose the GL 3.3 / 2.1 / software backend) |
| UI panels, dialogs, editor tools | `raygui` (bundled) with a custom dark tactical style, plus our own immediate-mode widgets for map-specific tools |
| Contours, graphics, symbols | Polylines/triangles through raylib's `rlgl` batch. Static layers (contours, vegetation hatching) are **cached in `RenderTexture`s per zoom band** and only redrawn when the view changes |
| Heatmap / hillshade / slope views | Texture generated at bake time + a small fragment shader (GLSL 330 and GLSL 100 variants for GL 2.1/ES2) |
| NATO symbols | Built-in vector symbol set drawn from our own primitive definitions (frame + modifiers); `milsymbol` SVGs as visual reference only |
| Text | TTF monospace via `LoadFontEx`; SDF font shader for crisp zooming |
| Anti-aliasing | `FLAG_MSAA_4X_HINT` where supported; graceful without |

Sim/terrain crates:

| Need | Crate |
|---|---|
| Math | `glam` (f32 render) / f64 in sim where precision matters |
| Deterministic RNG | `rand_chacha` (seeded, one stream per subsystem) |
| Deterministic transcendental math | `libm` (avoid platform `sin/cos` differences if lockstep MP is ever wanted) |
| Parallelism | `rayon` (for read-only phases only; results merged in a fixed order) |
| Spatial index | `rstar` (R-tree) or own uniform grid |
| Pathfinding | own A*/HPA* over cost grid (`pathfinding` crate for prototypes) |
| Contours | `contour` crate (marching squares → GeoJSON-like rings) or own |
| Serialization | `serde`, `ron`, `toml`, `bincode`/`postcard` for saves/replays |
| Map authoring formats | `tiff` (pure-Rust GeoTIFF-compatible rasters), `geojson` (vector features) — so maps can also be edited in QGIS |
| Compression | `zstd` for baked runtime layers |
| Real-world import (later) | `gdal` behind an optional feature (needs system GDAL), `osmpbf` for OpenStreetMap, `proj` for projections |
| Noise / generation | own fBm/ridged/domain-warp implementation (deterministic, `libm`-based); `noise` crate for prototypes only |
| Testing | `cargo test`, `proptest`, `insta` (snapshot), `criterion` (benchmarks) |
| Profiling | `tracing` + `tracy` (`tracing-tracy`) |

Tooling: `rustup` stable, `cargo-nextest`, `clippy -D warnings`, `rustfmt`, `cargo-deny`, GitHub Actions CI matrix (ubuntu/windows/macos), `just` as the task runner.

### 3.3 Alternatives considered

| Stack | Verdict |
|---|---|
| C++20 + raylib + Dear ImGui | Most established all-native option; not chosen (owner prefers Rust). |
| Rust + Bevy | Rejected: breaking releases every ~3–4 months, and its wgpu renderer prefers Vulkan/DX12/Metal (weak on 2013 GPUs). |
| Godot 4 + C# | Rejected: a full engine, more than we need; GLES3 renderer is fine on old GPUs but the engine is heavyweight. |
| Plain SDL3 | Covered by raylib's SDL platform backend; SDL3's GPU API needs modern graphics APIs. |
| TypeScript + PixiJS/WebGL | Weakest for a heavy multi-threaded sim. A WASM build of `sim` could power a web viewer later. |

---

## 4. Simulation architecture

### 4.1 Core loop
- **Fixed timestep, 20 Hz** (50 ms) sim tick. Vehicle dynamics sub-stepped (e.g., 4×) if needed.
- Subsystems run at their own multiples of the base tick:

| Subsystem | Rate |
|---|---|
| Vehicle dynamics, projectiles | 20 Hz (+ substeps) |
| Gunnery / crew actions | 20 Hz |
| Sensor detection checks | 4 Hz |
| Vehicle AI (behaviour tree / utility) | 5 Hz |
| Platoon AI (planning, re-evaluation) | 1 Hz + event-driven |
| Commander-level OPFOR AI | 0.2 Hz + event-driven |
| Reporting / comms delivery | event-driven with delays |

- Pause = stop stepping. Time compression = more ticks per frame (never larger dt).
- Renderer interpolates between the last two sim states.

### 4.2 API boundary (the most important rule)
```rust
sim.submit(cmd: Command)                     // player/AI orders, timestamped at tick
sim.step() -> Vec<Event>                     // advance one tick
sim.perceived(side) -> &PerceivedWorld       // what this side KNOWS (fog of war)
sim.truth() -> &TruthWorld                   // only for AAR, debug overlay, tests
```
The client renders `PerceivedWorld` for the player's side. Truth access in the client is behind a debug feature flag. AI for a side also reads only that side's `PerceivedWorld` — **the AI does not cheat** (configurable for difficulty, but off by default).

### 4.3 Determinism
- Same seed + same command log ⇒ identical outcome. Replays are just `(scenario, seed, command log)`.
- No `HashMap` iteration order in sim logic (use `BTreeMap`/`IndexMap` or sorted vectors). No wall-clock time. No thread-order dependence.
- A CI test replays golden command logs and asserts a state hash.

### 4.4 Entity model
Component-style data in plain structs-of-arrays inside `sim` (no engine ECS): `Transform`, `Hull`, `Turret`, `Mobility`, `Armour`, `Weapons`, `Ammo`, `Crew`, `Sensors`, `Radio`, `DamageState`, `AiState`, `Membership(platoon/section)`. This keeps infantry/artillery additions additive.

---

## 5. Terrain & map

### 5.1 Map data, generation and editor
**Full design in `docs/TERRAIN.md`**: map package format (authoring vs. baked), the procedural generation pipeline, the terrain editor, real-world import, and how rivers, forests, urban areas, roads and obstacles are modelled.

Summary: 4 m base elevation grid plus deterministic procedural micro-relief; raster layers for land cover, vegetation, soil and water depth; vector features for rivers, roads, hedgerows, buildings and bridges. Features modify the terrain at bake time (rivers carve, roads cut and fill). Authoring files are GeoTIFF + GeoJSON, so QGIS works as a second editor.

### 5.2 Rendering the map (tactical display)
- **Base (default: heatmap + contours together)**: muted single-hue elevation heatmap or hypsometric tint, optional hillshade, with contours drawn on top. Each layer can be toggled independently. Computed once at bake into a texture.
- **Contours**: marching squares on the elevation grid; index contour every 50 m (thicker, labelled), intermediate every 10 m (scale-dependent density), supplementary 5 m at high zoom. Pre-extracted per zoom level, rendered as polyline meshes.
- **Overlays**: 1 km grid (MGRS-style labels), vegetation as hatching/stipple (not solid colour), roads as thin double lines, water as outline + light fill.
- Toggle modes: *Contours*, *Heatmap*, *Slope*, *Trafficability*, *Dead ground from point*, *Viewshed from unit*.

### 5.3 Line of sight
- **Point-to-point LOS**: march along the ray over the heightfield (DDA over grid cells, bilinear height), compare with the sight line between eye height (e.g., gunner's primary sight ~2.3 m; commander unbuttoned ~3 m) and target sample heights. Earth curvature negligible at ≤5 km but trivial to add.
- **Partial visibility**: test several target sample points (track line, hull top, turret ring, turret roof, sight head) → visible fraction per aspect. This is the basis of hull-down detection and of the hit model (visible target area).
- **Buildings**: urban tiles carry a 1 m surface-height layer (terrain + buildings), so rays use the surface height there instead of bare earth. See `TERRAIN.md` §5.
- **Vegetation**: Beer–Lambert-style attenuation through canopy (`p_see = exp(-k * density * path_length)`), resolved deterministically by a hash of (observer, target, time bucket) so it doesn't flicker.
- **Viewsheds**: R2/XDraw-style radial sweep for single observers; GPU or rayon-parallel for many. Hierarchical max-height pyramid for early-out.
- **Budget**: ~14 vs 30 vehicles at 4 Hz ≈ 1,700 LOS rays/s × ~1,500 cells — trivial. Viewsheds for AI position search are the heavy part → cached, computed on worker threads, invalidated only when inputs change.

---

## 6. Sensors, detection and fog of war

### 6.1 Detection model
Detection is **probabilistic over time**, not binary on LOS:
- Per observer-sensor/target pair, compute the number of resolvable cycles across the target's critical dimension (sensor resolution, magnification, range, atmospheric attenuation, visible fraction).
- Map cycles → probability via **Johnson criteria / Targeting Task Performance (TTP) curves** for *detection → recognition (tank vs. not) → identification (which type) → IFF (friend/foe)*.
- Time-to-detect uses an exponential search process (ACQUIRE model style) where the rate rises with signature.
- Modifiers: target moving, firing (muzzle flash/blast/dust), dust plume from movement on dry soil, engine heat (thermal), thermal crossover at dawn/dusk, smoke (visual blocked; thermal mostly not), rain/fog, observer workload (gunner tracking a target is not searching), buttoned vs. unbuttoned commander, crew fatigue/suppression.
- Sensors per vehicle: unaided eye (unbuttoned TC), periscopes, gunner's primary sight (day + thermal), commander's independent sight (hunter-killer), image intensifiers for older designs.

### 6.2 Reported picture (what the player sees)
1. Crew detects → **vehicle's local track** (with classification confidence).
2. Vehicle sends a **spot report** (contact report / SALUTE-style) over the radio net → **delay** (crew workload, net congestion; e.g., 2–15 s) and **position error** (bearing/range error depending on whether lased).
3. Platoon/company **track picture** fuses reports into tracks with: last known position, time since last seen, type confidence, estimated heading/speed.
4. Tracks **age**: icons fade, become "last known position" ghosts, then expire. Duplicate tracks are possible (two tanks reporting the same enemy → may show as two contacts until correlated).
5. **Misidentification is possible** (including friendly vehicles reported as enemy → fratricide risk under poor ID conditions).

Player display shows only this fused picture. Own units' positions are also reported (near-real-time if the vehicle has a digital position system; voice-position-report lag for older eras — era-dependent).

---

## 7. Vehicle physics & combat model

### 7.1 Mobility
- **Longitudinal**: engine power curve → transmission (simplified gear/efficiency or CVT-like hydrokinetic) → sprocket tractive effort, limited by traction (`μ × weight` from soil class); resistances: rolling (soil-dependent), grade (`m·g·sinθ`), turning. Integrate speed. Braking limits.
- **Steering**: skid-steer turn rate limited by power and soil (regenerative steering for modern hulls); pivot turn where applicable.
- **Terrain**: speed limits by slope, side slope (rollover limit), vertical step/trench crossing limits, fording depth, tree-knockdown (vegetation override force vs. stem diameter), soft soil immobilisation risk.
- **Reference model**: NATO Reference Mobility Model (NRMM) concepts — speed-made-good from soil strength (cone index), slope, obstacles, vegetation — at a simplified level. Bekker–Wong terramechanics only where needed.
- Fuel consumption (idle vs. moving; gas turbine vs. diesel difference is noticeable and gameplay-relevant).
- Hull pitch/roll from terrain affects gun elevation/depression limits — **this is why hull-down positions on reverse slopes need gun depression** (e.g., Western tanks ~−9° to −10°, Soviet-lineage ~−4° to −6°: a doctrinally important asymmetry).

### 7.2 Turret, sights and fire control
- Turret traverse rate and gun elevation rate (powered/manual), gun stabilisation quality (fire-on-the-move accuracy), laser rangefinder, ballistic computer, crosswind/cant sensors, lead computation, muzzle reference system, thermal sight generation.
- Engagement sequence timeline modelled explicitly: detect → lay → lase → compute → fire → observe → re-engage. Times drawn from crew skill distributions. Reference crew drill times: TC 3-20.31 (crew gunnery) engagement standards.
- Loading: human loader (fast first rounds, then fatigue; ready-rack vs. hull stowage) vs. autoloader (fixed cycle, carousel capacity). Ammo stowage location matters for damage.

### 7.3 Ballistics & hit probability
- Projectile flight: point-mass trajectory with drag (or tabulated velocity-decay per ammo type; APFSDS loses on the order of tens of m/s per km). Time of flight matters for moving targets.
- **Hit probability via error budget**: total dispersion = √(Σσ²) over ammunition dispersion, ranging error, residual ballistic error (temperature, wear), stabilisation/tracking error, lead error vs. target manoeuvre, crosswind, cant. Each round is sampled from a 2D normal around the aim point and intersected with the **actual visible target silhouette** at that aspect — so hull-down really is harder to hit.
- First-round vs. adjusted fire: subsequent rounds correct bias.

### 7.4 Terminal effects
- **Armour**: per-vehicle armour as zones (front hull upper/lower, turret front, sides, rear, roof, belly) with *separate* RHA-equivalents vs. KE and vs. CE (composite and ERA differ hugely between the two). Compute line-of-sight thickness from impact obliquity.
- **Penetration**: KE long rods via **Lanz–Odermatt** formula (velocity, length/diameter, density, target hardness); HEAT/CE via jet penetration vs. stand-off (published penetration figures, degraded by ERA/spaced armour); HE/HE-FRAG for soft components.
- **Behind-armour effects / damage**: ray continues into a simplified internal layout (crew stations, ammo, fuel, engine, transmission, gun/breech, FCS, radios, tracks/sprockets). Spall cone. Outcomes: **M-kill, F-kill, K-kill (catastrophic), crew casualties, subsystem degradation** (e.g., thermal sight out, turret drive on manual, radio out). Ammo compartment isolation (blow-out panels) vs. carousel cook-off.
- Non-penetrating hits still matter: optics/sight damage, track damage, crew shock and suppression.

### 7.5 Crew model
- Four (or three with autoloader) crew members with skill, fatigue, morale/suppression. A crew casualty degrades the role (e.g., TC moves to gunner station).
- Commander buttoned vs. unbuttoned: much better situational awareness vs. vulnerability to fragments/small arms (and artillery later).
- Suppression from near misses and hits reduces detection rate, increases engagement times, and can trigger AI break-contact behaviour.

### 7.6 Vehicle data policy
All vehicles are **fictional designs mirroring real ones** (e.g., Blue "M1A1-like", "Leopard 2A4-like"; Red "T-72B-like", "T-80U-like"). Each parameter in `data/vehicles/*.ron` carries `source`, `confidence` (high/medium/low/estimate) and `notes`. Armour figures are always estimates from open sources; document the range and the chosen value. **Public sources only** — never seek or use restricted/classified data.

---

## 8. Command & control

### 8.1 Hierarchy
```
Player (Company commander)  ── issues intent-level orders + graphics
   └─ Platoon AI (PL)      ── decomposes into section/vehicle tasks, picks positions, distributes fires
       ├─ Section A (PL + wingman)
       └─ Section B (PSG + wingman)
           └─ Vehicle AI (TC) ── drives, searches, engages, executes drills
                └─ Crew sim (driver, gunner, loader) — timings, skill
```
Standard platoon: 4 tanks — PL (tank 1) with wingman (tank 2), PSG (tank 4) with wingman (tank 3).

### 8.2 Order delay
Orders are transmitted over the radio net with a delay (composition + transmission + subordinate's acknowledgement + planning time). Pre-planned orders with **code words / triggers** execute instantly — rewarding planning, as in ATF.

### 8.3 Command set (v1)

**Movement**
| Order | Parameters |
|---|---|
| Move / Tactical road march | route (waypoints or "AI choose covered route"), formation, movement technique, speed, halt conditions |
| Change formation | column, staggered column, wedge, vee, line, echelon left/right, coil (halt), herringbone (halt) |
| Movement technique | traveling, traveling overwatch, bounding overwatch (alternate / successive) |
| Halt / Resume | — |
| Follow / Follow-and-support | another platoon |

**Positions & defence**
| Order | Parameters |
|---|---|
| Occupy battle position | **area polygon** (AI finds positions), position type: *hide / turret-down / hull-down*, orientation/primary sector, alternate & supplementary positions (auto), occupation method (deliberate/hasty) |
| Assign sector of fire | left/right limits (TRPs), engagement priority |
| Engagement area | polygon, trigger line, TRPs, fire pattern (frontal / cross / depth), fire technique (simultaneous / alternating / observed) |
| Overwatch / Support by fire | area to occupy + area/target to cover |
| Screen / observe | observation area, "report only, do not engage" |
| Displace / Move to alternate | auto or on trigger ("displace after N shots / when engaged") |

**Offence**
| Order | Parameters |
|---|---|
| Attack (hasty) / Assault objective | objective polygon, axis or direction of attack, formation |
| Attack by fire | position area, target area |
| Bypass / Reconnoitre route | — |
| Consolidate / Reorganise | on objective; triggers LACE/ACE reports |

**Fires & weapons control**
| Order | Parameters |
|---|---|
| Weapons control status | HOLD / TIGHT / FREE |
| Engagement criteria | max range, target types, "engage only if ≥ N targets" |
| Fire at target / area | (micro-order, allowed but rare) |
| Smoke | self-screening grenades (auto in drills) |
| Recon by fire | suspected position |

**Drills (triggered automatically by AI, can also be ordered)**
Action on contact (react → develop the situation → choose a course of action → execute → report), react to direct-fire contact, react to an obstacle, break contact, react to indirect fire, react to air attack (later), occupy BP.

**C2 / misc**
Button up / unbutton, silent watch (engine off, thermals on battery), report status (ACE/LACE), rally point, phase lines and triggers ("when 2nd PLT crosses PL RED, 1st PLT moves"), contingency orders.

### 8.4 Control measures the player can draw
Phase line, boundary, objective, battle position, engagement area, TRP, trigger line, checkpoint, contact point, assembly area, attack position, line of departure, limit of advance, axis of advance, direction of attack, route, rally point, obstacle, sector of fire.
Symbology per **MIL-STD-2525D / APP-6D** tactical graphics.

---

## 9. AI system

The AI is the heart of "command, don't control". Approach: **hierarchical, doctrine-driven, deterministic**, using proven game-AI techniques rather than learned policies (which are opaque, hard to tune, and do not explain themselves in radio reports).

### 9.1 Layers and techniques
| Layer | Technique | Responsibility |
|---|---|---|
| Commander (OPFOR & optional Blue auto-commander) | **HTN planner** with doctrine templates + scenario plan; utility scoring to choose among COAs; optional shallow MCTS over abstract COAs later | Allocate platoons to tasks, re-plan on major events |
| Platoon | **HTN decomposition** of orders into section/vehicle tasks; **tactical position evaluation**; fire-distribution assignment (Hungarian algorithm over target-to-shooter utilities) | Where each tank goes, when sections bound, who shoots what |
| Vehicle | **Behaviour tree + utility considerations** | Drive the route, keep formation, search sectors, engage, micro-reposition (pop up, fire, back down), execute drills |
| Spatial reasoning (shared) | **Influence / threat maps**, cached viewsheds, cost grids | Exposure, cover, dead ground, likely enemy avenues |

### 9.2 Example: "Occupy hull-down positions in this area"
1. Player draws area + primary sector/EA. Platoon AI requests threat directions (from EA, known tracks, likely avenues of approach).
2. **Candidate sampling**: ~200–1000 points in the area (Poisson-disc), plus micro-search around promising ones (1–5 m steps, several headings).
3. **Per-candidate evaluation** (parallel, cached):
   - *Hull exposure*: for observers along threat directions, minimal visible height of the hull → hull-down if hull hidden and gun/sight line clear; turret-down if only sights clear.
   - *Gun depression check*: hull pitch at the spot vs. required depression to cover the EA.
   - *Coverage*: fraction of EA visible from sight height and within effective range.
   - *Flank exposure*: visibility from non-threat directions.
   - *Skyline*: avoid topographic crest silhouette (prefer military crest).
   - *Covered approach & egress*: route exposure cost to get there and to an alternate.
   - *Concealment*: vegetation, background.
4. **Assignment**: choose 4 positions maximising combined utility with constraints: lateral dispersion (~100–300 m), mutual support, interlocking sectors, section integrity. Greedy + local improvement or small ILP.
5. Also select **alternate** and **supplementary** positions and **hide positions** behind each.
6. Explain: the PL reports *"Red One, set in BP 12, hull-down, sector TRP 3 to TRP 5"*; debug overlay shows candidate scores.

### 9.3 Pathfinding & movement
- Cost grid from trafficability + slope + vegetation + **exposure to known/suspected threats** (covered vs. fastest route).
- A* on a hierarchical grid (HPA*) for long routes; smoothing (string pulling) then local steering with formation slots.
- Formation movement after Pottinger's "Coordinated Unit Movement": a virtual leader follows the path, slots are offsets, vehicles adapt to terrain (they don't stay pixel-perfect).
- Bounding overwatch implemented as an HTN method: section A overwatches from a selected position (via §9.2 evaluator) while section B bounds to the next one.

### 9.4 Doctrine as data
Formation spacings, bound lengths, engagement ranges, fire patterns, drill thresholds, decision latency, initiative level and the HTN method library live in **doctrine packs** (`data/doctrine/<pack>/`). A scenario assigns a pack to each side; any pack can be used by either side.

- `western-combined` (Blue baseline): ATP 3-20.15 / FM 17-15 — bounding overwatch, hull-down defence, mission command, high subordinate initiative.
- `generic-opfor` (Red baseline): a neutral, plausible opposing doctrine — echeloned attack, march → pre-battle → battle formation deployment, tighter centralised control, lower subordinate initiative.
- Mods: e.g., `soviet-1985`, `russian-2015` packs (TC 7-100.x, FM 100-2-1, Grau) using the same schema.

Packs can also carry era-dependent variants, e.g., a modern pack that assumes a shared digital picture.

### 9.5 What we deliberately do not do (v1)
- No reinforcement-learning policies in the game loop. (Possible later use: offline tuning of utility weights using the headless harness.)
- No AI access to ground truth by default.
- **[OPTIONAL, later]** Natural-language order entry ("2nd platoon, occupy BP 3 hull-down, sector TRP 4 to 6") parsed into the same structured command set — fun, but strictly a front-end to existing commands.

---

## 10. UI / UX

### 10.1 Visual language
- **Palette**: near-black blue/green background; terrain in one or two desaturated hues; foreground lines in pale green/amber. Affiliation colours per 2525/APP-6 but desaturated: friendly cyan-blue, hostile red, unknown yellow, neutral green. Colour is never the only cue (frame shape distinguishes affiliation: rectangle friendly, diamond hostile, quatrefoil unknown).
- **Typography**: monospace, uppercase labels (e.g., *IBM Plex Mono*, *JetBrains Mono*, *Share Tech Mono*; all OFL-licensed).
- **Effects**: minimal. Optional faint scanline/phosphor glow toggle. Shell traces as short thin lines; hits as small flash marks; destroyed units as symbol with an X.
- **Symbols**: NATO frames at platoon level when zoomed out, individual vehicles when zoomed in (aggregation by zoom). Implemented natively as vector primitives (the needed 2525D/APP-6D subset); **milsymbol** (MIT) is a visual reference.

### 10.2 Screen layout (desktop, 1600×900 minimum)
Borrowed from RTS conventions (StarCraft / Command & Conquer): every order is a visible button that also shows its hotkey, so the game is learnable with the mouse alone and fast with the keyboard.
```
┌──────────────────────────────────────────────────────────────────────────────┐
│ TOP BAR: clock · state · EXECUTE / PAUSE · X1 X2 X4 X8 · AAR │ map layers · editors · HELP │
├────────────┬─────────────────────────────────────────────────┬───────────────┤
│ ORDER OF   │              (tool prompt, centred)              │ SELECTION     │
│ BATTLE     │                                                  │ platoon task, │
│ (roster)   │                 TACTICAL MAP                     │ route, member │
│            │   contours · grid · graphics · routes + CPs      │ rows (click → │
│            │                                                  │ single tank)  │
├──────────┬─┴──────────────────────────────────────┬──────────┴───────────────┤
│ MINIMAP  │ RADIO NET (terse, timestamped)          │ COMMAND CARD 5 × 3       │
│          │                                         │ (sub-card while placing) │
├──────────┴─────────────────────────────────────────┴──────────────────────────┤
│ STATUS: cursor grid, elevation, slope, land cover · layer · m/px               │
└──────────────────────────────────────────────────────────────────────────────┘
```
Command card rows: orders (move, route, halt, battle position, engage), posture (formation, technique, weapons control, timing, next platoon), planning tools (phase line, objective, TRP, viewshed, LOS). While a route, phase line or battle position is being placed, the card swaps to a sub-card (send/finish, undo point, radius, cancel). Hovering any button shows a tooltip with its hotkey.

### 10.3 Interaction
- Select a platoon (map, roster or Tab) → right-click the map to move, or pick an order on the command card and click the map (targeting mode; right click or Esc cancels). Timing (K) makes the next order a contingency (on enemy crossing the last PL, or in 5 min).
- **Routes with checkpoints**: Shift+right-click adds checkpoints (appending to the platoon's current route) and the route is sent as one order when Shift is released; or the ROUTE tool: left-click checkpoints, right-click the destination. One radio order carries the whole route (`via` on `Move`/`PlatoonMove`); the platoon plans leg by leg, keeps formation through the checkpoints and, after a contact drill, resumes with only the checkpoints not yet passed.
- Orders are visible as graphics on the map (dashed = sent / not yet acknowledged, solid = executing), with numbered checkpoints and an X at the destination.
- **Auto-pause** options: on first contact, unit destroyed, unit requests orders, order complete.
- Tools: LOS check (point-to-point with profile chart), viewshed from point at chosen eye height, range ruler (metres + mils + bearing), dead-ground shading.
- Mouse or keyboard: space = pause, 1–4 speed, a hotkey for every card button, Tab cycles platoons; clicking a selected roster entry centres the map; minimap click/drag pans.
- **After-action review**: timeline scrubber, ground truth toggle, LOS lines of every shot, statistics (detections first, rounds fired, hits, Pk by range).

### 10.4 Audio
Sparse: radio clicks, squelch, terse synthesised or recorded callouts (optional), distant gunfire. No music during play (optional menu music).

---

## 11. Other features to plan for (not v1 unless noted)

- **Scenario editor** (v1, basic): place units, draw graphics, set weather/time, triggers, victory conditions.
- **Terrain editor + procedural generator** (v1): see `docs/TERRAIN.md`. **Real-world import** (DEM + OSM) in v1.x, same format.
- **Saves** (v1): snapshot of sim state; replays via command logs.
- Weather and time of day (v1: day/night/dusk, clear/rain/fog); dust; thermal crossover.
- Smoke (v1 self-screening; artillery smoke later).
- Obstacles & minefields (v1 static; breaching later).
- Indirect fire / call for fire (v1.x).
- Recovery & resupply (v1.x).
- Campaign with persistent crews and vehicle damage (v2).
- More unit types (v2+): IFVs, dismounts, ATGM teams, recon, UAS.
- Multiplayer lockstep (v3?).
- Modding: everything data-driven in RON; documented schemas; doctrine packs, era packs, vehicle packs as drop-in folders.
- Accessibility: colour-blind-safe palette, UI scale, rebindable keys.
- Localisation-ready strings.

---

## 12. Validation & testing strategy
- **Unit tests** for formulas (penetration, ballistics, LOS), with known reference values in test comments.
- **Property tests** (LOS symmetry under equal heights, monotonic Ph vs. range, etc.).
- **Golden replays**: deterministic hash checks in CI.
- **Validation harness** (`headless`): Monte-Carlo duels at range bands; compare Ph/Pk and engagement times against published data and historical outcomes (e.g., 73 Easting 1991, Golan 1973 — qualitatively). Results written to CSV for plotting.
- **AI scenario tests**: scripted test maps asserting e.g. "occupy hull-down in area X ⇒ all 4 tanks hull-down vs. threat direction within 120 s".
- **Benchmarks** for LOS/viewshed/position search.

---

## 13. Roadmap (milestones)

| # | Milestone | Exit criteria |
|---|---|---|
| M0 | Skeleton | Workspace, CI, raylib window (GL 3.3 + GL 2.1 builds), map package format, first procedural heightfield, contours + heatmap, pan/zoom |
| M0.5 | Terrain v1 | Generation pipeline stages 1–6 (TERRAIN.md §3), editor basics (height brushes, draw river/road/forest), bake, analysis views |
| M1 | One tank moves | Mobility model on terrain, Move order, route on cost grid, pause/time compression |
| M2 | Seeing | LOS (incl. partial), viewshed tool, detection model, spot reports, perceived picture, fog of war |
| M3 | Shooting | FCS timeline, ballistics, Ph error budget, armour/penetration, damage; duel validation harness |
| M4 | Platoon | Formations, movement techniques, Occupy BP with hull-down position finder, fire distribution, drills |
| M5 | Opponent | OPFOR AI with doctrine templates; first full scenario playable start-to-finish |
| M6 | Commander UX | Planning phase, graphics, triggers, AAR replay, scenario editor, urban generation + editor polish |
| M7 | v1.0 | Cold War + Modern content sets, 5–8 scenarios, 4–6 vehicle types per era, packaging for Win/Linux/macOS |
| M8 | v1.x | Real-world import (DEM/OSM), indirect fire, resupply |

### 13.1 Implementation status (2026-10-11)

| Milestone | Done | Deferred (tracked here) |
|---|---|---|
| M0 | Workspace, CI (Linux full + Windows/macOS build/test), GL 3.3 + GL 2.1 client builds, authoring/baked map format, procedural heightfield, contours (5/10/50 m), relief tint + hillshade, pan/zoom, 1 km grid | Contour/layer `RenderTexture` caching per zoom band (direct drawing is fast enough so far); TTF monospace font (raylib default font for now) |
| M0.5 (lean) | Generation stages 1–3, 5, 6 (TERRAIN.md §3); baker with river carving, road cut/fill, forest rasterisation, mobility speed rasters; editor: raise/lower/smooth/flatten brushes on the edit layer, draw river/road/forest, undo/redo, save, re-bake on a worker thread (UI never stalls; edits during a bake restart it, latest wins); analysis views (slope, GO/SLOW-GO/NO-GO, land cover) | Stage 4 stamps, stage 7 validation, lakes, hedgerows, settlement interiors; tiled/incremental bake (full re-bake on stroke release, ~0.2 s for 8 km plus the layer image, now off the UI thread); `veg_density`/`canopy_height` rasters; raygui panels |
| M1 | Tracked-vehicle mobility (power/traction/resistance on grade, soil, terrain caps, skid-steer yaw limits, fuel); Move/Halt orders; fastest-route A* on the baked speed raster with string pulling; pure-pursuit route following; pause and ×1/×2/×4/×8 time compression (more ticks); render interpolation; radio log; `PerceivedWorld` for own units; golden replay hash | Order delay (§8.2), reverse driving, HPA* (full-resolution A* is ~fast enough on 8 km maps), formation/platoon movement (M4) |
| M2 | LOS over ground + micro-relief with partial visibility (5 target heights), Beer–Lambert canopy attenuation, earth curvature; viewshed + LOS tools; Johnson/TTPF + ACQUIRE detection with era-filtered sensors (optical/night channel/thermal) and light level; vehicle tracks → delayed, noisy spot reports → fused contacts that age and expire; fog of war via `ai::picture::PerceivedWorld` (no truth ids) | Misidentification/fratricide, thermal crossover, smoke, dust, season-dependent daylight, era-dependent own-position lag, LOS speed target (~50 µs at 4 km vs. 20 µs goal; pairs are cached) |
| M3 | Fire-control timeline (traverse, lay, lase, fire, reload, adjusted fire), linear-decay ballistics, Ph error budget sampled against the visible silhouette, Lanz–Odermatt KE penetration (constant `c` calibrated, see `data/combat/penetration.ron`), per-face armour with obliquity, data-driven damage tables (M/F/K-kill, crew, sights), duel harness (`tanks-headless duel` → CSV) | HEAT/CE rounds and ERA, internal component model and spall, ammunition types per target, coaxial MG, fire-on-the-move doctrine differences |
| M4 | Platoons from scenario data; doctrine packs (formation slots, spacing, bound length, delays, engagement range, drills); formation movement with virtual leader and slot re-planning; traveling / traveling overwatch / bounding overwatch; Occupy BP with hull-down finder (coarse grid + 3 m micro-search, threat observers, gun depression, dispersion, assignment), crews report position type; frontal fire distribution; react-to-contact drill (halt, orient, engage, resume when quiet); order delay over the radio | Alternate/supplementary/hide positions, displacement on trigger, skyline/military-crest scoring, break-contact drill, smoke, cross/depth fire patterns, HTN method library |
| M5 | Scenario objectives, victory conditions (force destroyed, objective seized and held 2 min, time limit with defender tie-break) and per-side AI plans; commander AI (`ai::commander`, 0.2 Hz) reading only the side's picture: attack template march → pre-battle → battle formation → consolidate with break-off on losses, defend template; orders go over the radio with delay; scenario `m5-hold-the-village` playable start to finish (also headless with both AIs) | Re-planning/COA choice (HTN/utility), reserves and counter-attacks, OPFOR reconnaissance, artillery preparation, Blue auto-commander option in the UI |
| M6 | Planning phase (clock stopped at H-hour; orders, graphics and contingencies apply at once, ENTER executes); control measures drawn in the client (phase lines, objectives, TRPs, battle positions) and held in each side's picture; triggers (enemy crosses PL, platoon crosses PL, at time) that release orders as code words; replays = scenario + player command log (`sim::Replay`, CTRL+R in the AAR, `tanks-headless replay` verifies the hash); AAR with timeline scrub over recorded truth frames, shots and per-side stats (rounds/hits/kills/losses, first contact, hits by range band); scenario editor (F9: place/drag/rotate/delete units, platoons, vehicle by in-service year, objectives, date, start time, save to `data/scenarios/<id>-custom.ron`, play); generated village buildings (footprints in `buildings.geojson`, baked to a height grid that blocks LOS and movement); terrain editor building tool and feature delete | Town blocks/streets beyond villages, building damage, AI use of buildings, AAR replay by re-simulation (frames are recorded live), graphics in the AAR, editing AI plans and victory conditions in the scenario editor, trigger/graphic editing after placement, PL-relative report text |

**UX pass (2026-10-12, after M6):** clickable top bar (time control, map layers, editors, help), order-of-battle roster, selection panel with clickable vehicle rows, StarCraft-style command card with hotkey labels, tooltips and placement sub-cards, minimap, wrapped radio net; multi-checkpoint routes (sim `via`, see §10.3). Deferred: box/multi-platoon selection, control groups, a real order queue (sequential *different* orders, e.g. move then occupy BP), auto-pause options, LOS profile chart.

**Suggested start for M7 (v1.0):** content first. Add Modern-era vehicles and sensors (thermal sights, APFSDS generations) as data with sources, 5–8 scenarios across both eras using the scenario editor, then packaging (release builds per OS with `data/` alongside). Keep re-running the duel harness when adding vehicles.

---

## 14. Decisions log
| # | Topic | Decision (2026-10-11) |
|---|---|---|
| 1 | Engine | **Rust sim core + raylib 6.x client** via `raylib-rs`; GL 3.3 default, GL 2.1 / software fallbacks |
| 2 | Era | **Cold War → modern → near future**, era as data (§2.3). v1 content: Cold War (~1985) + Modern (~2005) |
| 3 | Player echelon | Company team, up to 14 tanks |
| 4 | Maps | **Procedural first** + built-in terrain editor; real-world import later in the same format (`docs/TERRAIN.md`) |
| 5 | Red doctrine | Generic data-driven OPFOR; Soviet/Russian-style as a doctrine pack/mod |
| 6 | Map display | Heatmap **and** contours, independently toggleable |
| 7 | Licence | **Open source, MIT** for code, data and docs (`LICENSE`). Bundled third-party assets keep their own licences (e.g., fonts under OFL) and are listed in `THIRD_PARTY.md` |
| 8 | Language | **Rust** |
| 9 | Name | **Tanks** (working title) |
| 10 | Baked layers | Single zstd block per layer for now instead of 256² tiles; quantised elevation (2 cm) is applied in memory too, so a loaded bake equals a fresh bake bit-for-bit |
| 11 | Bridges | A road over a river is an implicit bridge: water removed for at least 1.5 cells either side of the road centreline. Explicit bridge structures with MLC come with TERRAIN.md §5.2 |
| 12 | Display images | Analysis/base images are computed on the CPU in `terrain::shade` and uploaded as textures, so the client ships no shaders yet (rule 7 trivially met) |
| 13 | Generator randomness | `terragen` uses position-keyed hashing per stage (no RNG streams), so per-pixel stages run on threads with identical results |
| 14 | Replay input | A replay holds only externally given commands (`generated == false`) plus the sides a human played; AI and trigger orders are regenerated by re-simulation. Control measures carry no radio delay and draw no random numbers, so `Sim::settle` may apply them during the planning phase without changing replay results |
| 15 | Buildings | Footprints are authored features; the baker rasterises them to a building-height grid (`buildings.bin`) used by LOS (opaque to roof height) and as NO-GO in every mobility raster |
| 16 | Multi-point routes | A route with checkpoints is **one** order (`via: Vec<Vec2>` on `Move`/`PlatoonMove`, omitted from RON when empty so old replays load unchanged), not a client-side queue of moves: it crosses the radio once, replays deterministically and the platoon AI owns resumption after contact. Legs are planned separately (`Pathfinder::find_via`) so checkpoints are actually visited |

### Remaining open questions
None blocking. Development environment: see `docs/DEVELOPMENT.md`.
