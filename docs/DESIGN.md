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
- Infantry, ATGM teams, aviation, UAS/drones, engineers, logistics vehicles. (IFVs and recon vehicles enter in M7b; artillery arrives only as *fire support* called by the player, M7d: no gun lines are simulated.)
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
│   ├── client/     # raylib app: menus, game, terrain + scenario editor screens. Talks to sim via Commands/Events/PerceivedWorld
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
Orders are transmitted over the radio net with a delay (composition + transmission + subordinate's acknowledgement + planning time). Pre-planned orders with **code words / triggers** execute instantly — rewarding planning, as in ATF. A **checkpoint plan** (§8.3) crosses the net once, like any order; what it says to do later (waits, code words, branches) then needs no further transmission. A **code word** is a control signal: sent once, it reaches every platoon of the side at once and releases every plan and contingency waiting on it (FM 6-0 execution by code word).

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

**Plans (checkpoint plans, 2026-10-14)**
A platoon's plan is a route of checkpoints (FM 1-02.2, ADP 3-90) in which each leg and each checkpoint carries its own instructions, edited on the map (§10.3) and sent as one `SetPlan` order (§14 #22). The platoon AI runs it at 1 Hz by issuing its existing orders (§14 #23).
| Part | Contents |
|---|---|
| Leg (to the CP) | speed (MAX or a doctrine march speed), formation, technique, weapons control, reaction to contact; unset = keep what the platoon has. Settings apply as the lead section passes the previous CP |
| At the CP | actions in order: face a bearing, occupy a BP (hull-down / turret-down, facing; the plan holds until the BP is reported set), smoke screen, engine smoke on/off, tank smoke rounds at a point, call for fire (HE / smoke), report |
| Wait | hold at the CP until a condition set is met (ANY / ALL): code word, elapsed time, clock time, enemy crosses a PL, enemy in an objective/BP/TRP, another platoon has reached one of its CPs. A CP with no actions and no wait is a pass-through: the formation flows through it |
| On release | the same actions, run when the wait is met and before the next leg (the "on HAMMER: smoke, fire, go" of a synchronised action) |
| Branch | while on the leg to the CP or waiting at it: contact within a range, under fire, strength at most N, (any wait condition) → continue at another CP (FM 6-0 branch) |
| Contact reaction | per leg or as a quick order: DRILL (default, halt-orient-engage-resume), CONTINUE (bypass, fire on the move), HASTY POSITION (hull-down ground close by facing the threat), BREAK CONTACT (the battle drill; ends the plan) |
Conditions read the side's picture only (rule 3). A quick move, BP or break-contact order replaces the plan; HALT pauses it and RESUME continues the same step; posture orders keep it. Code-word names come from the doctrine pack (`code_words`).
*Realism.* Execution on a code word, checkpoints, phase lines and event triggers are real control measures (FM 6-0 "on order" and the execution matrix, ADP 3-90). Real orders are mostly intent plus a few such measures, and subordinates use initiative (mission command): a short plan with contingencies is more realistic than a long rigid script, so the defaults leave the platoon AI to act on its own (contact drill on). Simplifications in v1: a code word reaches every platoon instantly and is never missed (no acknowledgement, no jamming) and readiness is reported perfectly; Tier 2 communications effects (§13.2) can add delay and missed transmissions through the same `CodeWord` path.

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
│ TOP BAR: clock · state · EXECUTE / PAUSE · X1 X2 X4 X8 · AAR │ map layers · HELP · MENU    │
├────────────┬─────────────────────────────────────────────────┬───────────────┤
│ ORDER OF   │              (tool prompt, centred)              │ SELECTION     │
│ BATTLE     │                                                  │ platoon: task,│
│ (roster)   │                 TACTICAL MAP                     │ strength, rows│
│ FIRE SUPP. │                                                  │ plan: steps,  │
│ CODE WORDS │                                                  │ CP settings   │
│            │   contours · grid · graphics · routes + CPs      │ tank: status  │
│            │                                                  │ card · DATA   │
├──────────┬─┴──────────────────────────────────────┬──────────┴───────────────┤
│ MINIMAP  │ RADIO NET (terse, timestamped)          │ COMMAND CARD 8 × 3       │
│          │                                         │ (sub-card while placing) │
├──────────┴─────────────────────────────────────────┴──────────────────────────┤
│ STATUS: cursor grid, elevation, slope, land cover · layer · m/px               │
└──────────────────────────────────────────────────────────────────────────────┘
```
Selection panel: a platoon shows its vehicle types, task, formation, route and a strength line (vehicles up, main-gun rounds, lowest fuel), then one row per vehicle; clicking a row selects that vehicle alone. A single vehicle shows its status card, what its crew reports: condition and crew fit, task (and hull-down note), grid and heading, speed, fuel (% and litres), main-gun rounds and gun state (ready / laying / lasing / reloading), turret bearing and weapons control, current target (from the crew's own track: type, range, direction) and sights in service (or damaged). DATA (I) opens the DATABASE on that vehicle type. Own units are reported exactly (§6.2), so the card reads `PerceivedWorld` only.

Command card rows: orders (move, route, halt, battle position, engage), posture (formation, technique, weapons control, timing, move speed; Tab selects the next platoon), planning tools (phase line, objective, TRP, viewshed, LOS), and an eighth column for PLAN. While a route, phase line or battle position is being placed, the card swaps to a sub-card (send/finish, undo point, radius, cancel); while a plan is edited, to the plan sub-card (SEND, ADD CPS, PREV/NEXT CP, CP MENU, DELETE CP, TIMING, REVERT). Hovering any button shows a tooltip with its hotkey.

Left column under the roster: FIRE SUPPORT (batteries) and CODE WORDS: each word that a sent plan or a contingency waits on, with the platoons waiting on it and their state from their own reports (`1 SET · 3 MOVING · 4 LATER`, `ALL SET`); clicking a word, or Ctrl+1…9 by its place in the doctrine list, sends it. While a platoon with a plan is selected, the selection panel lists its CPs with ETAs, then the selected CP's settings; each row opens that part of the CP menu.

### 10.3 Interaction
- Select a platoon (map, roster or Tab) → right-click the map to move, or pick an order on the command card and click the map (targeting mode; right click or Esc cancels). Timing (K) makes the next order a contingency (on enemy crossing the last PL, or in 5 min).
- **Routes with checkpoints**: Shift+right-click adds checkpoints (appending to the platoon's current route) and the route is sent as one order when Shift is released; or the ROUTE tool: left-click checkpoints, right-click the destination. One radio order carries the whole route (`via` on `Move`/`PlatoonMove`); the platoon plans leg by leg, keeps formation through the checkpoints and, after a contact drill, resumes with only the checkpoints not yet passed.
- **Right click and the context menu**: a plain right click on the ground still moves the selection, acting on release; holding the button 250 ms, right-clicking an object (plan CP or leg, own unit, contact, graphic) or right-clicking with nothing selected opens a cascading context menu at the cursor. Its items dispatch the same actions as the command card (ground: move here / at speed, route via, add CP to plan, BP, smoke rounds, call for fire, draw, LOS, viewshed, timing, orders; unit: select, data, edit/clear plan, on contact, orders; contact: engage, fire, smoke, LOS; graphic: move to, fire, delete). Keyboard: arrows, Enter / Right open, Left / Esc back, letter accelerators; greyed rows say why (NO BATTERY, NO SMOKE ROUNDS). Options: RIGHT CLICK = MOVE | MENU (menu first, MOVE HERE focused), PAUSE ON MENU (the clock stops while a menu is open or a plan is edited: TacOps-style WEGO on demand), PAUSE WHEN SET (the clock stops when every platoon waiting on a code word is set).
- **Plan editing** (§8.3): PLAN (P) starts a draft from the plan last sent, the running plan, or the current route. Left clicks add CPs, a CP drags to move, Del deletes the selected CP, `,` / `.` step through them, C opens the CP menu (LEG TO CP ▸ speed / formation / technique / weapons / on contact; AT CP ▸ actions; WAIT ▸; ON RELEASE ▸; IF ON LEG ▸ condition ▸ GO TO CP; MOVE / INSERT / DELETE CP; START HERE; CLEAR PLAN); right click on a leg inserts a CP or sets the leg. Items ending in "…" take a map click (bearing or target). The draft is drawn dashed in the draft colour, a sent plan dashed until acknowledged, a running plan solid: numbered CP squares, a march speed as ticks across the leg (not colour only), a mid-leg label when zoomed in (`25 KM/H · WEDGE · TRAV OVW`), badges (actions, `WAIT HAMMER`, `THEN SMOKE`), BP symbols, fire-mission marks, a dotted arrow to a branch target labelled with its condition, and an ETA per CP (TacOps): the planning speed raster capped by the leg's speed or the formation speed, `?+m:ss` after a wait with no time bound. SEND (Enter) issues the plan; editing a running plan sends it with `start` = the current CP so it keeps its progress; REVERT (Esc) drops the draft.
- **Halt / resume** is one toggle (H): HALT stops in place and keeps the interrupted order — a platoon's route with its checkpoints and the sections' progress, an unfinished battle-position order, the move a contact drill would have resumed, or a single vehicle's own route. The route stays drawn, the task reads HALTED, ROUTE HELD and the button becomes RESUME, which continues from where the unit stopped. Any new move or BP order discards the held one.
- **Move speed** (S) cycles MAX and the doctrine pack's ordered march speeds (`speed_limits_mps`, e.g. 40 / 25 / 15 km/h). It sets the speed of the next move and orders the selection to it (`SetSpeed`, also on a move in progress or held). A speed is a ceiling in km/h, not a fraction of top speed, so a mixed platoon keeps together: the virtual leader and every vehicle, catching up included, stay under it, and terrain can still hold them below it. MAX is the formation speed (60 % of the slowest member's cross-country speed).
- Orders are visible as graphics on the map (dashed = sent / not yet acknowledged, solid = executing), with numbered checkpoints and an X at the destination.
- **Auto-pause** options: on first contact, unit destroyed, unit requests orders, order complete.
- Tools: LOS check (point-to-point with profile chart), viewshed from point at chosen eye height, range ruler (metres + mils + bearing), dead-ground shading.
- Mouse or keyboard: space = pause, 1–4 speed, a hotkey for every card button, Tab cycles platoons; clicking a selected roster entry centres the map; minimap click/drag pans.
- **After-action review**: timeline scrubber, ground truth toggle, LOS lines of every shot, statistics (detections first, rounds fired, hits, Pk by range).

### 10.4 Screens and flow
The client is a set of screens, not one view with modes. The game and the two editors are separate screens, each with its own top bar, help (F7) and menu (Esc / F10); none of them can switch to another in place.
```
TITLE ──any key──▶ MAIN MENU ─┬─ PLAY SCENARIO ──▶ scenario list + briefing ──▶ LOADING ──▶ GAME
                              ├─ SCENARIO EDITOR ─▶ scenario list ──▶ LOADING ──▶ SCENARIO EDITOR ⇄ PLAY TEST (F9)
                              ├─ TERRAIN EDITOR ──▶ map list ──▶ LOADING ──▶ TERRAIN EDITOR
                              ├─ DATABASE (overlay)       vehicles · ammunition · terrain · doctrine · combat models
                              ├─ OPTIONS (overlay)        RESUME <game> when a game was left for the menu
                              └─ QUIT
```
- **Loading** generates/bakes/loads the map on a worker thread (progress lines from the generator), so the window never stalls; a failure shows the error and returns to the main menu.
- **Screen menu (Esc)** over any map screen: resume, screen entries (game: AAR, restart, back to the editor when play testing; terrain editor: save, bake; scenario editor: save, play test), options, help, main menu, quit. Esc first cancels whatever a tool is doing (targeting tool, drafted route, feature being drawn).
- Leaving a game for the main menu suspends it (RESUME on the main menu); starting a new scenario discards it. Leaving an editor with unsaved edits asks first.
- **Mission end**: a panel with the result and AFTER-ACTION REVIEW / RESTART / MAIN MENU / CLOSE.
- **Scenario select** shows date, map, sides (strength, doctrine, player or computer), objectives, victory conditions and the scenario's `briefing` text.
- **Database**: the data the simulation loads, read-only, in five tabs: vehicles, ammunition, terrain (land cover and soils), doctrine packs (with formation diagrams) and combat models (detection, penetration, damage). Every value shows its confidence (HIGH / MED / LOW / EST); hovering shows its source and notes. Values marked CALC are derived with the simulation's own functions (power-to-weight, ground pressure, velocity, drop and penetration by range, range at which each round defeats each vehicle's frontal armour, power-limited speed per soil). Opened from the main menu, the screen menu, or a vehicle's DATA button in the game; the game stays paused underneath.
- **Options** (saved at once to `<config dir>/tanks/settings.cfg`, or `$TANKS_CONFIG`): borderless fullscreen (also F11), frame-rate cap, UI font (IBM Plex Mono / built-in), pan speed, edge pan, contours and grid when a map opens.
- Every menu works with the mouse and with Up/Down/Enter/Esc; the visual language is the map's (contour backdrop, same panels and buttons).

### 10.5 Audio
Sparse: radio clicks, squelch, terse synthesised or recorded callouts (optional), distant gunfire. No music during play (optional menu music).

---

## 11. Other features to plan for (not v1 unless noted)

- **Scenario editor** (v1, basic): place units, draw graphics, set weather/time, triggers, victory conditions.
- **Terrain editor + procedural generator** (v1): see `docs/TERRAIN.md`. **Real-world import** (DEM + OSM) in v1.x, same format.
- **Saves** (v1): snapshot of sim state; replays via command logs.
- Weather and time of day (M7c: sun elevation from date/time/latitude, clear/haze/fog/rain/dust presets from data, wind, thermal crossover, dust plumes).
- Smoke (M7a, layered): vehicle self-screening grenades, engine exhaust smoke, tank smoke rounds, then artillery smoke (M7d). A drifting, decaying cloud field that attenuates each sensor band separately (§6.1).
- Obstacles & minefields (v1 static; breaching later).
- Indirect fire / call for fire (M7d, pulled forward from v1.x): fire support as a scenario asset, with suppression.
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
| M7a | Smoke | Self-screening, engine smoke, tank smoke rounds, drifting/decaying cloud field in LOS per sensor band, break-contact drill, SMOKE order |
| M7b | Ammo and targets | HEAT/HE-FRAG/APDS/ATGM and tank smoke rounds, CE vs. KE armour and ERA, IFV and recon vehicles, mixed OPFOR, ammo selection (AI and player). Coax MG deferred to M8 (no dismounts to use it on) |
| M7c (done) | Weather and time | Sun elevation, night/dusk/fog/rain/dust presets as scenario data, wind, thermal crossover, dust cue; scenarios that use them |
| M7d (done) | Fire support | Call for fire (HE, smoke), FDC delay, time of flight, dispersion, suppression model, react-to-indirect-fire drill, SHOT/SPLASH reports |
| Plans (done) | Plans and context menu | Checkpoint plans (per-leg settings, actions, waits, code words, branches, contact reaction) in the sim; right-click context menu; plan editing on the map with ETAs; code-word strip; the example plan runs to completion on the demo map and its replay verifies (`docs/plans/PLANS-AND-CONTEXT-MENU.md`) |
| M7e | v1.0 | Cold War + Modern content sets, 5–8 scenarios, 4–6 vehicle types per era, packaging for Win/Linux/macOS (the former M7) |
| M8 | v1.x | Real-world import (DEM/OSM), resupply, **Tier 2** (§13.2) |
| M9 | v1.5 | **Tier 3** (§13.2): speculative systems as data/era packs |

### 13.1 Implementation status (2026-10-11)

| Milestone | Done | Deferred (tracked here) |
|---|---|---|
| M0 | Workspace, CI (Linux full + Windows/macOS build/test), GL 3.3 + GL 2.1 client builds, authoring/baked map format, procedural heightfield, contours (5/10/50 m), relief tint + hillshade, pan/zoom, 1 km grid; UI text in IBM Plex Mono (TTF, one atlas per size, sized to the built-in font's advance so layouts hold; falls back to the built-in font) | Contour/layer `RenderTexture` caching per zoom band (direct drawing is fast enough so far) |
| M0.5 (lean) | Generation stages 1–3, 5, 6 (TERRAIN.md §3); baker with river carving, road cut/fill, forest rasterisation, mobility speed rasters; editor: raise/lower/smooth/flatten brushes on the edit layer, draw river/road/forest, undo/redo, save, re-bake on a worker thread (UI never stalls; edits during a bake restart it, latest wins); analysis views (slope, GO/SLOW-GO/NO-GO, land cover) | Stage 4 stamps, stage 7 validation, lakes, hedgerows, settlement interiors; tiled/incremental bake (full re-bake on stroke release, ~0.2 s for 8 km plus the layer image, now off the UI thread); `veg_density`/`canopy_height` rasters; raygui panels |
| M1 | Tracked-vehicle mobility (power/traction/resistance on grade, soil, terrain caps, skid-steer yaw limits, fuel); Move/Halt orders; fastest-route A* on the baked speed raster with string pulling; pure-pursuit route following; pause and ×1/×2/×4/×8 time compression (more ticks); render interpolation; radio log; `PerceivedWorld` for own units; golden replay hash | Order delay (§8.2), reverse driving, HPA* (full-resolution A* is ~fast enough on 8 km maps), formation/platoon movement (M4) |
| M2 | LOS over ground + micro-relief with partial visibility (5 target heights), Beer–Lambert canopy attenuation, earth curvature; viewshed + LOS tools; Johnson/TTPF + ACQUIRE detection with era-filtered sensors (optical/night channel/thermal) and light level; vehicle tracks → delayed, noisy spot reports → fused contacts that age and expire; fog of war via `ai::picture::PerceivedWorld` (no truth ids) | Misidentification/fratricide, thermal crossover, smoke, dust, season-dependent daylight, era-dependent own-position lag, LOS speed target (~50 µs at 4 km vs. 20 µs goal; pairs are cached) |
| M3 | Fire-control timeline (traverse, lay, lase, fire, reload, adjusted fire), linear-decay ballistics, Ph error budget sampled against the visible silhouette, Lanz–Odermatt KE penetration (constant `c` calibrated, see `data/combat/penetration.ron`), per-face armour with obliquity, data-driven damage tables (M/F/K-kill, crew, sights), duel harness (`tanks-headless duel` → CSV) | HEAT/CE rounds and ERA, internal component model and spall, ammunition types per target, coaxial MG, fire-on-the-move doctrine differences |
| M4 | Platoons from scenario data; doctrine packs (formation slots, spacing, bound length, delays, engagement range, drills); formation movement with virtual leader and slot re-planning; traveling / traveling overwatch / bounding overwatch; Occupy BP with hull-down finder (coarse grid + 3 m micro-search, threat observers, gun depression, dispersion, assignment), crews report position type; frontal fire distribution; react-to-contact drill (halt, orient, engage, resume when quiet); order delay over the radio | Alternate/supplementary/hide positions, displacement on trigger, skyline/military-crest scoring, break-contact drill, smoke, cross/depth fire patterns, HTN method library |
| M5 | Scenario objectives, victory conditions (force destroyed, objective seized and held 2 min, time limit with defender tie-break) and per-side AI plans; commander AI (`ai::commander`, 0.2 Hz) reading only the side's picture: attack template march → pre-battle → battle formation → consolidate with break-off on losses, defend template; orders go over the radio with delay; scenario `m5-hold-the-village` playable start to finish (also headless with both AIs) | Re-planning/COA choice (HTN/utility), reserves and counter-attacks, OPFOR reconnaissance, artillery preparation, Blue auto-commander option in the UI |
| M6 | Planning phase (clock stopped at H-hour; orders, graphics and contingencies apply at once, ENTER executes); control measures drawn in the client (phase lines, objectives, TRPs, battle positions) and held in each side's picture; triggers (enemy crosses PL, platoon crosses PL, at time) that release orders as code words; replays = scenario + player command log (`sim::Replay`, CTRL+R in the AAR, `tanks-headless replay` verifies the hash); AAR with timeline scrub over recorded truth frames, shots and per-side stats (rounds/hits/kills/losses, first contact, hits by range band); scenario editor (place/drag/rotate/delete units, platoons, vehicle by in-service year, objectives, date, start time, save to `data/scenarios/<id>-custom.ron`, play); generated village buildings (footprints in `buildings.geojson`, baked to a height grid that blocks LOS and movement); terrain editor building tool and feature delete | Town blocks/streets beyond villages, building damage, AI use of buildings, AAR replay by re-simulation (frames are recorded live), graphics in the AAR, editing AI plans and victory conditions in the scenario editor, trigger/graphic editing after placement, PL-relative report text |

**Database and vehicle card (2026-10-12):** DATABASE screen (§10.4) and the selected-vehicle status card (§10.2); the own-unit picture now carries crew fit, gun state, current target and sight damage. Deferred: filtering by scenario date, side-by-side comparison, silhouettes/profile drawings.

**Halt / resume (2026-10-12):** HALT keeps the interrupted order and the same button resumes it (§10.3); `Command::Resume`.

**Move speed (2026-10-13):** ordered march speeds from the doctrine pack (§10.3); `PlatoonMove.speed_limit_mps`, `Command::SetSpeed`. The NEXT PLT card button gave way to MOVE SPEED (Tab still cycles platoons). Deferred: separate catch-up speed, night/visibility defaults.

**Game shell (2026-10-12):** title screen, main menu, scenario select with briefing (`briefing` field in the scenario format), threaded loading screen, options (persisted), screen menu (Esc), mission-end panel, game suspended/resumed from the main menu; terrain and scenario editors moved out of the game into their own screens, the scenario editor play-tests and returns with F9 (§10.4). Deferred: new map from a recipe and new blank scenario from the menus, map preview in the select screens, campaign/scenario progress, key rebinding, UI scaling, audio volume (no audio yet).

**UX pass (2026-10-12, after M6):** clickable top bar (time control, map layers, editors, help), order-of-battle roster, selection panel with clickable vehicle rows, StarCraft-style command card with hotkey labels, tooltips and placement sub-cards, minimap, wrapped radio net; multi-checkpoint routes (sim `via`, see §10.3). **Multi-platoon orders (2026-10-14):** Shift+click adds a platoon to the selection (again removes it); the card reads `PLATOON 1 + 2`. The client fans the primary's platoon orders (move, battle position, formation, halt/resume, speed, weapons, smoke, ammo, break contact, contact reaction) out as separate commands, one per platoon, so the sim, radio and replays are unchanged; destinations keep each platoon's offset from the primary. The extras apply only while the primary stays selected. CLI `--select 1+2`. Deferred: box selection, control groups, plans for several platoons at once, graphics/radio lines for the followers, a real order queue (sequential *different* orders, e.g. move then occupy BP), auto-pause options, LOS profile chart.

**M7a smoke (2026-10-13):** `sim::smoke` cloud field (wind-advected, growing then fading discs; `SmokeField::transmittance` per band via analytic chord length) feeding `sensors::step` (a sensor under `blind_below` is blinded; a pair with every sensor blinded is not refreshed, so the track times out); data in `data/combat/smoke.ron` (`grenade_ms` from 1998 blocks thermal, `grenade` and `engine` do not) and per-vehicle `smoke_launcher`/`engine_smoke`; scenario `weather` (wind); doctrine `smoke_when_engaged` (crews self-screen ≤ 2 s after being engaged, ≥ 20 s apart) and `break_contact_distance_m`; commands SMOKE (Z), ENGINE SMOKE (E), BREAK CONTACT (X: smoke, then withdraw along a path away from the nearest contact; the contact drill is suspended while withdrawing); smoke discs on the map, status-card row, DATABASE section. Golden replay hash changed because smoke state is hashed and crews now self-screen. Deferred: tank smoke rounds (M7b), artillery smoke (M7d), wind in the scenario editor, smoke vs. mobility/AI route choice (AI does not yet plan around clouds).

**M7b ammunition and targets (2026-10-13):** a vehicle carries a loadout of round types (`armament.load`); `GunState.stock` is aligned with it, and rounds outside the side's service year have stock 0 (all kept if none are in service). `AmmoKind` = `Apfsds` (Lanz–Odermatt), `Apds` (flat × (v/v0)²), `Heat` (flat; also the ATGM: 200 m/s, 12 s reload, 4 km), `HeFrag` (flat, light armour only), `Smoke` (`AmmoSmoke{kind, clouds}`). Per-round `reload_s`, `max_range_m`, `preferred_vs`. Vehicles have a `class` (`mbt|ifv|recon`); armour has `ce_factor` (chemical rounds meet armour × factor) and optional `era` (tiles, coverage 0..1 on front/sides only, `ce_defeat` fraction of penetration removed, `ke_gain_mm` added against kinetic rounds; each hit on a tile spends it; tiles filtered by in-service dates). Round choice (`gunnery::select_round`): the player's held type (`Command::SelectAmmo`, AMMO / Q) if stocked and in range, else the first stocked type whose `preferred_vs` includes the target class with penetration margin ≥ 1.0 (ERA allowed for), else the best margin; smoke is never auto-selected. Smoke rounds are a fire mission (`Command::FireSmoke`, SMOKE RND / N: aim point, 1–2 rounds per vehicle, spread 40 m square to the line of fire); the projectile's `Effect::Smoke` adds clouds on impact. New data: HEAT-MP 120, HEAT/HE-FRAG 125, 30 mm APDS and HE, SK5 ATGM, tank smoke shell (WP: radius 20 m, 70 s); vehicles SKIF M1 (IFV mirroring BMP-2) and FALCON R1 (recon mirroring FV107); the kurgan gains ERA from 1988. Scenario `m7b-mixed-force` (Red: one tank platoon, two IFV platoons; Blue: tanks plus a recon section). Golden replay hash changed (stock per type and ERA are hashed, round choice changed). Calibration note: with these numbers HEAT-MP cannot defeat the kurgan's frontal CE protection (676 mm) and only the ATGM can, unless ERA fires; in the mixed scenario Blue wins with no losses, so the IFV threat needs tuning with the validation harness before M7e. Deferred: coax MG (M8), AI preference for ATGM range, ammo resupply, HE vs. soft targets (no dismounts yet).

**M7c weather and time of day (2026-10-13):** `ScenarioDef.weather` = `{preset, latitude_deg (default 50), moon_phase, wind_*}`; presets live in `data/combat/weather.ron` (clear, haze, overcast, rain, fog, dust: visibility, thermal-band extinction, sky light factor, dust cue, precipitation), an unknown id refuses to load. `sim::weather` computes the sun elevation from date, local (solar) time and latitude (declination from day of year, hour angle; ±0.6° on the solstice noon checks), then the `Atmosphere` the sensors use: optical effectiveness from `DetectionModel.light_curve` (log-linear in sun elevation, civil/nautical/astronomical twilight anchors) plus moonlight (`moon_gain` × phase × the unlit-night level) with the preset's sky factor weighted by darkness; thermal contrast dips around sunrise/sunset (`thermal_crossover`: Gaussian in sun elevation, centre 8°, depth 45 %); and per-band transmittance `exp(-β·r)` (visible β = 3.912/V, thermal from the preset), multiplied with smoke, with the same blind threshold. Night channels (active IR) are not light-dependent but are blinded by fog/dust like any visible-band sensor; thermals see through dust and haze but not fog (droplets are comparable to 8–12 µm). A fast mover (> 4 m/s) raises a dust plume that boosts spotting by eye (not by thermal) on dry presets. `PerceivedWorld.weather` (`WeatherView`) feeds the top bar (sky, preset, visibility). Scenarios `m7c-night-attack` (02:00, half moon, Blue thermals vs Red active IR) and `m7c-fog-at-dawn` (fog, 06:30). Golden hash unchanged (clear-air extinction at V = 40 km changes hazards slightly but not the outcome of that duel); the light schedule `light_day/twilight/night` is gone, so the table in the database sheet shows the curve. Both new scenarios are won by Blue with no AI nuance for the Red side (it attacks blind into fog); needs tuning before M7e. Deferred: rain and fog effects on mobility, a weather editor in the scenario editor, weather changing during a scenario, day rollover for the sun beyond a simple day counter.

**M7d fire support (2026-10-13):** `data/combat/fires.ron` holds battery types (`sph155_btry`, `sph152_btry`: tubes, min/max range, FDC delay, volley interval, time of flight `base + per_km·range`, range σ as a fraction of range, deflection σ in mrad, HE and smoke rounds), HE effects by vehicle class (a direct zone ≤ 6 m and a near zone ≤ 35 m, each an `Outcomes` table, plus a sights-damage chance) and the suppression model. A side gets batteries from `SideDef.fire_support` (`BatteryAssign`: callsign, type, position, optional round counts); they are scenario assets, not map vehicles. `Command::CallForFire{battery, at, kind (He|Smoke), volleys}` and `CeaseFire`; `sim::fires` runs the mission: accepted (ETA = FDC delay + flight time) → SHOT (once) → SPLASH (5 s before each volley lands) → impacts → ROUNDS COMPLETE, or a `DenyReason` (out of range, no rounds, busy). Each round scatters with the battery's σ plus a 25 m sheaf spread across tubes; unadjusted, so a 4 km call misses by about 16 m and a 15 km call by about 60 m. Artillery smoke adds an `arty_wp` cloud. Suppression is a 0..1 state per vehicle: a burst adds up to 0.35 (linear to 60 m), a direct-fire miss 0.12, a non-penetrating hit 0.3; it decays 0.03/s and cuts detection cycles by up to 50 % and slows laying/reload by up to 80 %. Platoon drill "react to indirect fire" (burst ≤ 150 m in the last 8 s, 60 s cooldown): a moving platoon clears its speed limit, a static one holds; it emits `PlatoonReport::IndirectFire`. Impacts are kept 30 s and appear in a side's picture only within 1500 m of an own unit; each picture lists only its own batteries. Client: FIRE SUPPORT panel, CALL FIRE tool (Y), mission marker and impact rings, DATABASE sheets (HE effects, suppression, batteries), scenario `m7d-fire-support`. Golden hash changed (suppression is hashed; misses and non-penetrating hits now suppress). All effect numbers are `estimate`. **Commander AI fire support (2026-10-14):** doctrine `fire_support` (`danger_close_m`, `volleys`, `repeat_s`, `max_contact_age_s`; absent = never) makes `ai::commander` call HE on the fresh contact nearest to its own forces that a ready battery can reach and that is not danger close (`Order::CallFire`, sent over the radio like any order; tests in `m7d.rs`). **Adjusting fire (2026-10-14):** each battery draws a systematic error at its first volley (`range_bias_frac`, `deflection_bias_mrad` in `fires.ron`, on top of the random error), so unregistered fire is consistently off. `Command::AdjustFire{battery, shift, volleys}` moves the last aim point by the observer's correction and fires after the shorter `adjust_delay_s`; refused with `NoMission` or `Busy`. Client: ADJUST (J) takes the point where the last observed burst group should have landed and sends the difference, using the picture's impacts only. Golden hash unchanged (no battery in it). Deferred: AI smoke missions, AI adjusting, counter-battery, ammunition resupply of batteries, battery reload and relocation, danger-close checks, fire support in the scenario editor.

**Plans and context menu (2026-10-14):** `docs/plans/PLANS-AND-CONTEXT-MENU.md` P0–P5. Sim: `ai::picture::{Plan, PlanStep, LegSettings, StepAction, ConditionSet, Branch, ContactReaction}`, `Command::{SetPlan, SetContactReaction, CodeWord}`, `TriggerCondition` gains `CodeWord`, `Elapsed`, `ContactWithin`, `EnemyInArea`, `StrengthAtMost`, `UnderFire`, `PlatoonAt`; `sim::plan` runs plans at 1 Hz through the existing orders (arrival = a mobile vehicle within 60 m of the CP plus its formation slot offset, so a line or a platoon that lost its leader still arrives); `PlatoonView.plan`, reports `AtCheckpoint`, `PlanBranch`, `PlanComplete`, event `CodeWordSent`; doctrine `code_words`. Client: right click on release, hold-to-open context menu (`game/ctx.rs`, `hud::PopupMenu`), plan editing (`game/planedit.rs`), code-word strip with readiness and auto-pause (`game/codewords.rs`), AAR timeline marks for code words, radio text (`1: CP 2. WAITING HAMMER.`, `HAMMER: ALL SET.`, `1: UNDER FIRE, MOVING TO CP 4.`). Contingency triggers are now labelled CONTINGENCY, not CODE WORD. CLI `--plan`, `--plan-draft`, `--code-word`, `--rclick`; `docs/plans/example-plan.ron`. Golden replay hash unchanged. Deferred: single-vehicle plans (a lone vehicle keeps `Move` + `via`), nested condition logic, combining conditions from the menu (the data takes ANY/ALL lists; the menu sets one), plan templates / playbook, an execution-matrix screen, OPFOR use of plans, code-word acknowledgement and communications effects (Tier 2), keyboard placement of CP points (map points need the mouse), ETA for wait-time beyond Elapsed/AtTime. Observed while testing: a wedge routed through a village spends minutes re-planning its trailing slots around buildings (formation movement, not plans); worth a look before M7e content uses villages as CPs.

**Suggested start for M7 (v1.0):** the Tier 1 systems (M7a–d) come first, because they change what a scenario can say (night, fog, smoke, a mixed enemy, supporting fires) and so what content is worth authoring. Then M7e: Modern-era vehicles and sensors (thermal sights, APFSDS generations) as data with sources, 5–8 scenarios across both eras using the scenario editor, then packaging (release builds per OS with `data/` alongside). Keep re-running the duel harness when adding vehicles.

### 13.2 Feature tiers (review of 2026-10-13)

Prioritised from a review for two audiences: the casual player who likes military sims (wants readable tension and variety within a few minutes) and the enthusiast (wants doctrine and physics to be recognisable and sourced).

**Tier 1, M7a–d (in progress).** Smoke; ammunition variety and a mixed target set; weather and time of day; indirect fire with suppression. These make every existing system matter more (smoke breaks LOS, so fire distribution and positions change; night makes thermal sights decisive; HEAT/ERA make ammo choice and target type matter).

**Tier 2, M8.** Company-level tools: a real order queue and phase-line-driven plans; engineer obstacles and breaching, minefields and mine plough/roller; ATGM teams and dismounts (the first non-tank classes, using the component model of §4.4); cheap UAS reconnaissance feeding the picture; electronic and communications effects on reporting; resupply and recovery.

**Tier 3, M9 (speculative, as data/era packs, never code branches).** Active protection (hard-kill with a finite magazine, soft-kill jammers); top-attack munitions and the counters they force; loitering munitions; directed-energy dazzlers; unmanned ground vehicles as platoon members; multispectral camouflage and signature management; networked fire control (one platoon's contact handed off to another's gun). Each carries `estimate` confidence plus an explicit "speculative" note in the database screen.

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
| 17 | Client screens | The game, the terrain editor and the scenario editor are separate screens reached from the main menu (`client/src/mapscreen.rs`), not modes toggled inside one view: each has its own top bar, help and menu, and an editor never restarts a running game. The only in-place switch is the scenario editor's play test (F9 there and back). Map loads run on a worker thread. Options are a plain `key = value` file in the user's config directory (no new crate, unknown keys ignored) |
| 18 | Smoke model | Smoke is a sim-side list of 2D clouds (centre, radius, density, age) advected by the scenario wind and decayed over time; it is part of `state_hash`. Line of sight is unchanged: `sim::smoke` integrates chord length x extinction through the clouds along the sight line, separately for the visual/near-IR band (optical, night channel) and the thermal band. A pair whose every sensor is blocked is simply not refreshed, so tracks time out and engagement stops without special cases |
| 19 | Ammunition | A vehicle carries a loadout of round types (data), not one round. `AmmoKind` gains Heat, HeFrag, Apds (flat penetration) and Smoke; armour gains optional chemical-energy values and ERA. Selection is by target class (AI) or an explicit command (player). Non-rod rounds use flat penetration, with no Lanz–Odermatt term |
| 20 | Weather and light | A scenario carries a `weather` block (preset id from `data/combat/weather.ron`, wind). Light comes from sun elevation computed from date, time and map latitude (replacing the fixed hours), so season matters. Mobility is not weather-dependent yet: baked speed rasters cannot change at run time |
| 21 | Fire support | Fire support is a scenario asset per side (missions available, rounds, FDC delay), not simulated gun lines. A call for fire is a `Command`; impacts are scheduled projectiles with their own RNG stream, hashed. Suppression is a per-crew state that degrades search, aiming and willingness to move |
| 22 | Plans are one radio order | `Command::SetPlan` crosses the net once (radio delay as any order; immediate in the planning phase), like #16 for routes. The platoon AI owns the plan state (cursor, phase, passed steps) so it survives the contact drill, HALT/RESUME and replays; an edit sent with `start` = the step being executed keeps progress |
| 23 | Plan runner reuses orders | A plan step adds no movement code: `sim::plan` (1 Hz, platoon phase) turns the current step into the existing platoon orders (move with pass-through CPs as `via`, speed, formation, weapons, occupy BP) applied directly, and actions that need the sim (smoke, fire missions, break contact) as generated commands applied right after the phase; replays regenerate them (#14). Only player or trigger orders discard a plan |
| 24 | One condition vocabulary | Plan waits and branches use the same `TriggerCondition` as contingency triggers, extended; every condition reads the side's `PerceivedWorld` only. Conditions combine as a flat ANY/ALL list: no nesting, loops or variables in v1. A code word counts only if sent after the wait began (a transmission, not a state) |

### Remaining open questions
None blocking. Development environment: see `docs/DEVELOPMENT.md`.
