# Terrain — Map Format, Procedural Generation, Editor & Features

Status: **DRAFT for review**. Companion to `DESIGN.md` §5.

Terrain is the main character of this game: who sees whom first decides most tank fights. This document covers:
1. how a map is stored,
2. how maps are generated procedurally,
3. the terrain editor,
4. real-world import (later),
5. how each feature type (rivers, forest, urban, roads, …) is modelled for **LOS, mobility, cover/concealment and display**.

---

## 1. Core principle: author → bake → run

```
 ┌───────────────┐   ┌─────────────────────┐   ┌──────────────┐   ┌───────────────────────┐
 │ Generator     │──▶│  AUTHORING PACKAGE  │──▶│    BAKER     │──▶│   BAKED PACKAGE       │──▶ sim + client
 │ (recipe+seed) │   │ editable, open fmts │   │ deterministic│   │ runtime rasters,      │
 │ Importer      │──▶│ GeoTIFF + GeoJSON   │   │ incremental  │   │ indices, contours,    │
 │ Editor        │◀─▶│ + edit stack        │   │ per tile     │   │ cost & LOS pyramids   │
 └───────────────┘   └─────────────────────┘   └──────────────┘   └───────────────────────┘
```

- **Authoring package** = the source of truth. Non-destructive: a base heightfield plus *features* (rivers, roads, forests, towns) plus *edit layers* (brush strokes). Uses open formats so QGIS and other GIS tools can edit it too.
- **Baker** = a pure function `(authoring package) → baked package`. Features modify the terrain *at bake time* (a river carves its channel, a road cuts and fills its corridor), so moving a river in the editor re-carves the valley automatically.
- **Baked package** = what the sim loads. Compact binary, zstd-compressed, tiled. Never hand-edited.
- Generator, importer and editor all write the **same authoring format**. Procedural and real-world maps are therefore identical to the rest of the game.

Determinism: same authoring package + baker version ⇒ bit-identical baked package (checked by hash in CI).

---

## 2. Map package format

### 2.1 Coordinates and resolution
- Local planar metric coordinates: origin at the south-west corner, x = east, y = north, metres. Real-world maps keep their source CRS (e.g., a UTM zone) and origin in the manifest for reference and display grids.
- Map size: up to 16 × 16 km.
- **Base grid: 4 m** (16 km → 4,000² cells; f32 elevation ≈ 64 MB uncompressed, much less as quantised u16 + zstd).
- **Tiles: 256 × 256 cells** (1,024 m). Enables incremental bake, sparse high-resolution layers and parallel processing.
- **Sparse fine layers: 1 m** only where needed (urban tiles, selected detail areas).
- **Micro-relief: not stored.** A deterministic function `h_micro(x, y, seed, terrain_class)` adds 0.3–1.5 m relief at 5–30 m wavelengths. It is evaluated by LOS, hull-down search and vehicle pitch. Reason: real hull-down positions come from folds well below DEM resolution.

### 2.2 Authoring package layout
```
maps/<map-id>/
├── map.toml                 # id, name, size, cell size, seed, recipe ref, CRS/origin (if real), baker version
├── recipe.ron               # procedural recipe (if generated): preset + stage parameters
├── rasters/                 # GeoTIFF (float/uint), one per layer, tiled internally
│   ├── elevation_base.tif   # generated or imported bare earth
│   ├── edit_height.tif      # sum of brush deltas (non-destructive)
│   ├── landcover.tif        # class ids (see §2.4)
│   ├── veg_density.tif      # 0–255
│   ├── canopy_height.tif    # dm
│   ├── soil.tif             # soil class ids
│   └── moisture.tif         # 0–255 base wetness (weather modifies at runtime)
├── features/                # GeoJSON FeatureCollections (local coords), each feature has a stable id
│   ├── rivers.geojson       # centrelines with per-vertex width/depth/bank props
│   ├── water_bodies.geojson # lakes, ponds, marsh polygons
│   ├── roads.geojson        # polylines: class, surface, width, embankment
│   ├── rail.geojson
│   ├── forests.geojson      # polygons: forest type, density, age/height, undergrowth
│   ├── linear_veg.geojson   # hedgerows, tree lines, windbreaks
│   ├── urban_areas.geojson  # settlement polygons: type, density, era style → generator input
│   ├── buildings.geojson    # footprints: height, construction class (generated or hand-made)
│   ├── structures.geojson   # bridges, dams, walls, embankments, ditches, power lines
│   └── edit_masks.geojson   # "keep / regenerate" regions
└── meta/
    └── edit_history.ron     # optional: editor operation log for undo across sessions
```

### 2.3 Baked package layout (runtime)
```
maps/<map-id>/baked/
├── manifest.ron             # hashes of inputs, baker version, layer list
├── elevation.tiles          # u16 quantised (2 cm steps) + per-tile offset, zstd
├── surface_height.tiles     # sparse 1 m: terrain + buildings (urban tiles only)
├── height_pyramid.tiles     # max-height mip chain for fast LOS early-out
├── landcover.tiles, vegetation.tiles (density, canopy height, stem class), soil.tiles, water_depth.tiles
├── mobility_cost/<vehicle-class>.tiles   # precomputed speed factors per mobility class
├── hpa_graph.bin            # hierarchical pathfinding graph
├── features.bin             # roads/rivers/buildings/bridges with R-tree index
├── contours/<band>.bin      # pre-extracted polylines per zoom band (5/10/50 m)
└── render/                  # heatmap, hillshade, slope textures (PNG), vegetation masks
```

### 2.4 Land-cover classes (initial set)
Each class is a data entry in `data/terrain/landcover.ron` with: display style, LOS parameters, mobility parameters, concealment, dust and seasonal variants.

| Group | Classes |
|---|---|
| Open | bare soil, ploughed field, grassland/pasture, low crops, **tall crops** (maize — seasonal concealment), heath/scrub, sand, rock, snow |
| Wooded | deciduous forest, coniferous forest, mixed forest, young plantation, orchard, vineyard, brush |
| Wet | marsh/bog, wet meadow, reed beds |
| Water | river, stream, lake/pond (depth from water layer) |
| Built | urban dense, urban sparse/village, industrial, farmstead |
| Linear (vector only) | road classes, rail, hedgerow, tree line, wall, ditch, embankment, power line |

---

## 3. Procedural generation pipeline

A **recipe** = preset + seed + per-stage parameters. Every stage is deterministic, seeded (a separate stream per stage), and can be re-run individually. The editor can regenerate one stage while keeping hand edits (masked regions are protected).

**Presets** (each is just a recipe file; mod-friendly):
`rolling-farmland` (North European plain), `wooded-uplands` (central-German style hills and valleys), `river-valley`, `steppe`, `desert-wadi`, `mountain-valley`, `mixed-test` (small, for AI tests).

### Stage 1 — Macro landform
- Noise stack: fBm + ridged multifractal + **domain warping** for natural shapes; a large-scale low-frequency "tilt/basin" field controls regional drainage direction.
- Parameters: relief amplitude (e.g., 30 m for farmland, 300 m for uplands), roughness, ridge vs. rolling mix, dominant orientation.

### Stage 2 — Erosion and hydrology
- **Depression filling / flow routing**: Priority-Flood (Barnes et al. 2014) + D8 or D∞ flow directions → **flow accumulation**.
- **Erosion**: a stream-power-law pass (Cordonnier et al. 2016 style) for realistic valley networks, then a light particle-based hydraulic erosion pass and thermal erosion (talus angle) for slopes.
- Output: eroded heightfield, flow accumulation, drainage network.

### Stage 3 — Rivers and water bodies
- Channels where flow accumulation > threshold; stream order (Strahler) determines class (ditch/stream/river).
- **Hydraulic geometry** (Leopold & Maddock): width ∝ Q^0.5, depth ∝ Q^0.4 with Q ∝ accumulated area → per-vertex width/depth. Meander amplification for low-gradient reaches.
- Lakes where filled depressions exceed a size; marsh where wetness is high and slope is low.
- Rivers are emitted as **vector features** (`rivers.geojson`) and carved at bake time, so they stay editable.

### Stage 4 — Micro-relief and landforms
- Configure `h_micro` per land class (fields smoother, forest floors and heath rougher).
- Stamp discrete tactically meaningful features: gullies on steep flanks, low ridges/swells, sunken lanes, field berms, quarries, old embankments. These matter a lot for hull-down and dead ground.

### Stage 5 — Soil, moisture and land cover
- **Topographic Wetness Index** (TWI = ln(a / tan β)) + distance to water + slope → moisture.
- Soil class from preset + elevation/slope (e.g., loam on plains, sandy on terraces, peat in wet hollows, rocky on steep slopes).
- **Land cover by suitability rules**: farmland on flat, well-drained soil; forest on steep/poor/wet-ish ground and far from settlements; meadows along rivers; marsh in wet hollows. Patchiness with noise.
- **Field parcel generation**: Voronoi / recursive subdivision aligned to slope and roads → field polygons with crop types; **hedgerows and tree lines** along some parcel boundaries (density per preset; high for bocage-style maps).
- Forest polygons get type (deciduous/conifer/mixed), age → canopy height, stem density, undergrowth; forest edges get irregular shapes and clearings, firebreaks and forest tracks.

### Stage 6 — Settlements and road network
- **Settlement sites** scored by: flat ground, near water but above flood level, river crossing points, valley junctions; spacing by preset (villages every 2–4 km in European farmland).
- **Road network**: connect settlements via least-cost paths (slope-penalised A*, prefer contour-following; bridges where crossing is cheapest), hierarchy main road → secondary → farm/forest track. Prune redundant links (relative-neighbourhood-graph style). Bridges and fords get placed as `structures`.
- **Settlement interiors** (v1: villages and small towns): street layout grown from the entry roads (organic for old cores, grid for newer parts), parcels, building footprints by style and era (farmsteads, terraced rows, apartment blocks, industrial sheds), heights and construction class.
- Railway (optional per preset) with embankments and cuttings.

### Stage 7 — Validation
Automatic checks with a report in the editor: river continuity (flows downhill to an edge or lake), every settlement reachable by road, bridges where roads cross rivers, no impossible slopes on roads, some hull-down candidate density (see §6.3), map edges usable as entry zones.

**Implementation:** `crates/terragen`, no graphics dependencies, runnable from the CLI (`headless gen --recipe rolling-farmland --seed 42 --size 12km`) and from the editor.

---

## 4. Terrain editor

The editor is a **mode of the game client** (same renderer, same map display, same LOS tools), not a separate application. Heavy jobs (generation, bake) also run from the CLI.

### 4.1 Tools
| Category | Tools |
|---|---|
| Generate | New map from preset; re-run any stage with new parameters/seed; regenerate inside a selection only; protect masks |
| Height brushes | raise/lower, smooth, flatten (to sampled height), level slope between points, noise, erode (local hydraulic), ridge/valley stamp, micro-relief strength |
| Water | draw/edit river (centreline + width/depth handles), stream, lake/marsh polygon, ford, dam |
| Vegetation | forest polygon (type, density, height, undergrowth), hedgerow/tree line, orchard, paint density/canopy height |
| Ground | paint land cover class, soil class, moisture |
| Transport | draw road (class, surface, width; auto cut/fill + embankment), rail, bridge (span, MLC capacity), track |
| Urban | settlement polygon (type, density, era style) → auto-fill streets/buildings; place/edit single buildings; walls |
| Obstacles | anti-tank ditch, berm, minefield area, wire (scenario layer can also add these) |
| Analysis views | slope, trafficability per vehicle class, flow accumulation, LOS from point, viewshed, **hull-down candidate heatmap vs. a chosen threat direction**, dead ground, contour preview |

### 4.2 Editor UX
- Same tactical display style as the game; tool palette + property panel (raygui) + a status line with cursor grid ref, elevation, slope, class.
- Undo/redo via a command stack; every operation is a serialisable command (also saved to `edit_history.ron`).
- **Live preview**: edits re-bake only affected tiles in the background; contours/heatmap update within ~1 s.
- Snap/align helpers: follow contour, snap to road/river, straighten.

### 4.3 Scenario editor
Separate layer on top of a baked map: units, deployment zones, control measures, OPFOR plan, triggers, weather/date/time, victory conditions. Stored in `data/scenarios/<id>/` and never changes the map package.

---

## 5. Feature modelling

For every feature: how it affects **LOS**, **mobility**, **protection/concealment**, how it is **displayed**, and what the **AI** should know.

### 5.1 Relief (hills, valleys, micro-terrain)
- **LOS**: bare-earth elevation + `h_micro` along the ray. Hull-down candidates are evaluated with micro-relief included.
- **Mobility**: slope limits (longitudinal ~60 %, side slope ~30–40 % depending on vehicle); speed reduction with slope; vehicle pitch/roll from terrain at the hull footprint (4–6 sample points).
- **Display**: heatmap + contours (index 50 m, intermediate 10 m, supplementary 5 m at high zoom), optional hillshade.
- **AI**: crest vs. **military crest**, reverse slope positions, dead ground, defilade — all derived from cached viewsheds.

### 5.2 Rivers, streams and water bodies
Stored as centreline + per-vertex width, depth, bank height/angle and bottom type (`gravel | sand | mud`). The baker carves the channel (trapezoidal profile with banks) into elevation and writes a `water_depth` raster.

- **Mobility — crossing a water gap requires:**
  1. **depth ≤ fording depth** of the vehicle (unprepared ~1.0–1.2 m; prepared deep fording or snorkel up to ~4–5 m for some designs, with preparation time in minutes and risk);
  2. **bank exit**: bank height/angle within the vehicle's step and slope limits;
  3. **bottom bearing**: mud raises the immobilisation risk.
  Otherwise the vehicle must use a **bridge** (if the bridge's MLC ≥ vehicle class and it's intact) or a **ford**.
- **Fords**: features with known depth/bottom, often where tracks cross.
- **Bridges**: structure with span, width, **MLC capacity**, construction class, destructible (scenario/engineers later). Pathfinding sees them as the only edges across non-fordable water. Doctrinally key terrain → AI treats them as chokepoints.
- **Lakes**: impassable except where shallow edges are fordable.
- **Marsh/bog**: very low soil strength → slow, high bog-down risk; tanks avoid it unless ordered.
- **LOS**: water does not block, but offers no cover; river valleys often create long, open fields of fire. Banks and riverside vegetation (gallery forest, reeds) do block.
- **Display**: river outline + subtle fill; streams as single lines; depth/fordability annotations at fords/bridges; bridge symbols with MLC label.
- **Seasons (later)**: water level, frozen rivers (ice thickness vs. vehicle weight).

### 5.3 Forests and vegetation
Forest properties per polygon/cell: type (deciduous/conifer/mixed), **canopy height**, **stem density** (stems/ha), **mean stem diameter**, **undergrowth density**, seasonal **leaf-on/leaf-off** factor.

- **LOS**: Beer–Lambert attenuation `T = exp(-k · ρ · L)`, where `L` is the ray's path length *inside the canopy volume*. Rays passing *above* the canopy (observer on a hill) are not attenuated. Consequences that fall out naturally:
  - a tank **5–15 m inside a forest edge** can see out (short in-forest path) while being hard to see;
  - you can't see through 200 m of forest, but you can sometimes see through a thin tree line, especially in winter.
  The pass/fail result per (observer, target) pair is resolved via a deterministic hash so it doesn't flicker frame to frame.
- **Detection**: forest background + shadow reduce visual contrast; thermals see through light foliage better than eyes but not through dense canopy.
- **Mobility** (NRMM-style vegetation override): a tank can push over stems below a diameter threshold (depends on vehicle mass/force); dense large stems force use of tracks/firebreaks or very low speed; undergrowth slows; track damage risk. Forest tracks are fast lanes.
- **Protection**: trees can prematurely detonate HEAT rounds (later); negligible against KE.
- **Linear vegetation (hedgerows, tree lines)**: vector features with height/thickness/density, rasterised into the vegetation layer at fine resolution. Strong LOS blockers, moderate mobility obstacles (bocage hedges with earth banks can stop or belly vehicles).
- **Tall crops** (maize in summer): low concealment for crew in the open, no LOS block for vehicles above the crop; mainly matters for infantry later.
- **Display**: topographic-map style — forest as fine stipple/hatching plus an edge line (not solid green), hedgerows as dotted lines, orchards as a regular dot grid. Light enough to keep contours readable.
- **AI**: forest edges = concealed positions; avoid moving through forest unless ordered; use forest tracks; treat dense forest as restricted terrain.

### 5.4 Urban areas
At tank scale, buildings matter as **LOS blockers, mobility obstacles and ambush terrain**. Tanks fighting alone in towns is doctrinally poor; the AI should know that.

- **Representation**: building footprints (polygons) with height and construction class (`wood | masonry | reinforced concrete | industrial steel`), streets as part of the road network, plus open courtyards and parks.
- **LOS**: urban tiles get a sparse **1 m surface-height layer** (terrain + buildings, with walls ≥ 1 m thick rasterised conservatively). LOS uses surface height there. Building-polygon ray tests are the exact fallback.
- **Mobility**: streets at reduced speed (narrow streets: column only, no turret traverse room — later), buildings impassable for v1 (later: tanks can push through light wood/masonry with damage and debris), rubble as slow terrain.
- **Destruction (v1.x)**: HE hits damage buildings; collapsed buildings → rubble (reduced height, passable slowly).
- **Protection**: being behind buildings blocks LOS; firing through buildings is not modelled in v1.
- **Display**: building footprints as thin outlines with light fill (dense centres merge into blocks when zoomed out); built-up area outline at low zoom.
- **AI**: urban = severely restricted terrain for tanks; prefer bypass; if ordered in, move in column with sections overwatching intersections; engagements happen at very short range.
- **Generation**: settlement polygon + type + density + era style → streets → parcels → footprints. Villages and small towns in v1; larger towns are possible but performance-capped.

### 5.5 Roads, tracks and rail
- **Vector features**: class (`motorway | main | secondary | track | forest track`), surface (`paved | gravel | dirt`), width, embankment/cutting profile.
- **Baking**: road corridors are flattened along their length (cut and fill), creating **embankments and cuttings**, which are very tactically relevant (cover, obstacles, LOS blocks).
- **Mobility**: speed bonus depending on class/surface; dirt tracks degrade in wet weather. Road movement makes noise and dust (detection).
- **Rail**: embankments/cuttings, crossings as passage points.
- **Display**: thin double lines for main roads, single line for secondary, dashed for tracks, railway with ticks.
- **AI**: road marches use roads; tactical movement leaves them. Junctions, bridges and cuttings are chokepoints.

### 5.6 Soil and trafficability
- Soil class → base strength (cone-index-like value) in `data/terrain/soils.ron`; moisture (map + weather + season) modifies it.
- **Vehicle Cone Index (VCI)** per vehicle class: if soil strength < VCI, the vehicle can bog down; speed and fuel use are affected well before that point.
- **Dust**: dry, fine soils + speed → dust plume (detection signature, can obscure sights).
- **Display**: trafficability overlay (GO / SLOW-GO / NO-GO), per vehicle class, shown in planning and the editor.

### 5.7 Obstacles and man-made features
- Anti-tank ditches, berms, walls, dragon's teeth, minefields (scenario layer), craters (dynamic).
- Each has mobility rules (step height, gap width vs. vehicle trench-crossing ability) and LOS height.
- Power lines and pylons: display only (relevant for aviation later).

### 5.8 Weather, season and time of day (runtime modifiers)
Applied on top of the baked map without re-baking: soil moisture (rain → mud), leaf-on/off, snow cover (mobility, thermal contrast, tracks visible), fog/rain visibility, light level and thermal crossover.

---

## 6. Runtime use of terrain

### 6.1 LOS queries
- Ray march over `elevation` (+ `h_micro`) with early-out via the **max-height pyramid**; urban tiles use `surface_height`; vegetation via attenuation.
- Batched per sensor tick, parallel with rayon, results merged deterministically.

### 6.2 Mobility and pathfinding
- Per **mobility class** (e.g., "heavy tank 60 t", "medium tank 45 t") a cost/speed raster is precomputed at bake time from slope, soil, vegetation, water and obstacles; weather applies a runtime multiplier.
- HPA* graph over 1 km tiles; local A* inside tiles; bridges/fords are explicit graph edges.
- **Exposure cost** (to known/suspected threats) is added at query time from the AI's threat map. It's dynamic, so it is not baked.

### 6.3 Tactical analysis caches
- Hull-down candidate points per tile (pre-scored for 8 or 16 compass threat sectors at bake time; refined at runtime against the actual threat).
- Viewshed cache keyed by (position cell, eye height); invalidated only by dynamic changes (smoke, destruction).
- Dead-ground maps on demand.

---

## 7. Real-world import (v1.x, same authoring format)

| Layer | Source | Notes |
|---|---|---|
| Elevation | National 1 m lidar (DE, FI, NL, DK, US 3DEP), Copernicus GLO-30, EU-DEM | Resample to 4 m; at 30 m sources, micro-relief and procedural detail add realism |
| Land cover | ESA WorldCover (10 m), CORINE (EU) | Map classes to ours via a lookup table |
| Canopy height | ETH Global Canopy Height (10 m) or national lidar DSM − DTM | |
| Roads, rail, water, buildings, forests | OpenStreetMap (PBF) | `highway=*`, `waterway=*`, `natural=water`, `landuse=forest`, `natural=wood`, `building=*` (+ `height` / `building:levels`) |
| River depth/width | OSM `width` where present; otherwise hydraulic geometry from flow accumulation | |
| Bridges | OSM `bridge=yes` + estimated MLC from road class | |

Pipeline: `headless import --dem … --osm … --bbox … → maps/<id>/` (authoring package) → open in the editor → fix up → bake. GDAL is an **optional** build feature so normal builds don't need it.

---

## 8. Performance targets
- Generate + bake a 12 × 12 km map: **< 60 s** on a 4-core 2013-era CPU (multithreaded), **< 10 s** on a modern 8-core.
- Incremental re-bake after an editor brush stroke: **< 1 s** for the affected tiles.
- Runtime memory for a 16 × 16 km baked map: **< 500 MB**.
- Point-to-point LOS: **< 20 µs** average at 4 km.

---

## 9. Implementation order
1. Map format + heightfield + bake skeleton + contours/heatmap rendering (M0).
2. Stages 1–2 (landform, erosion/hydrology) + height brushes + analysis views.
3. Rivers (Stage 3) + water mobility + bridges/fords.
4. Soil/land cover/forest/hedgerows (Stage 5) + vegetation LOS.
5. Roads (Stage 6a) + cut/fill.
6. Micro-relief tuning against hull-down tests (Stage 4).
7. Settlements/urban (Stage 6b) + 1 m surface layer.
8. Validation stage, editor polish, presets.
9. Real-world import.

---

## 10. References (terrain-specific)
- Barnes, Lehman & Mulla (2014), "Priority-Flood: An optimal depression-filling and watershed-labeling algorithm for digital elevation models".
- Cordonnier, Braun, Cani, Benes et al. (2016), "Large Scale Terrain Generation from Tectonic Uplift and Fluvial Erosion" (Eurographics).
- Galin et al. (2019), "A Review of Digital Terrain Modeling" (Eurographics STAR) — survey of generation methods.
- Génevaux et al. (2013), "Terrain Generation Using Procedural Models Based on Hydrology" (SIGGRAPH).
- Musgrave, Kolb & Mace (1989), "The Synthesis and Rendering of Eroded Fractal Terrains"; Musgrave in *Texturing & Modeling: A Procedural Approach* (ridged multifractal).
- Leopold & Maddock (1953), "The Hydraulic Geometry of Stream Channels" (USGS Prof. Paper 252).
- Beven & Kirkby (1979) — topographic wetness index (TOPMODEL).
- Parish & Müller (2001), "Procedural Modeling of Cities" (SIGGRAPH); Kelly & McCabe (2006), "A Survey of Procedural Techniques for City Generation".
- NATO AVT-248 NG-NRMM report (2018) — soil strength, vegetation override, slope.
- ATP 2-01.3 — OAKOC; GO / SLOW-GO / NO-GO terrain classification.
- Inigo Quilez's articles on fBm and domain warping (iquilezles.org).
