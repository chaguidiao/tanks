//! Baker: a pure function `(authoring package, data) -> baked package` (TERRAIN.md §1, §2.3).
//!
//! Steps: edited elevation → carve rivers → cut/fill roads → rasterise forests → buildings
//! (height, identity, entrances) → quantise elevation → water depth → per-class speed rasters →
//! contours → hashes.
//!
//! Simplifications in this version (see TERRAIN.md §9): layers are stored as single zstd blocks
//! instead of 256² tiles, and a road crossing a river acts as an implicit bridge.

use std::io::{Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};
use sim_data::TerrainData;

use crate::contour::{self, ContourKind, ContourLine, ContourSpec};
use crate::features::{BuildingClass, ForestKind, RoadClass};
use crate::hash::StableHasher;
use crate::mobility::{self, MobilityClass};
use crate::raster;
use crate::{AuthoringPackage, Features, Grid, Heightfield, MapManifest, Result, TerrainError, Vec2};

/// Bump when baker output changes for the same input.
pub const BAKER_VERSION: u32 = 4;

/// Elevation quantisation step (m) in the baked package.
pub const ELEVATION_STEP: f64 = 0.02;

#[derive(Debug, Clone, PartialEq)]
pub struct MobilityLayer {
    pub class: MobilityClass,
    /// Planning speed, units of `mobility::SPEED_QUANTUM` m/s.
    pub speed: Grid<u8>,
}

/// A building in the baked map (TODO H8): identity, construction and entrances, for infantry
/// occupancy and foot routes. Floors, capacity and protection come from the class's entry in
/// `data/terrain/buildings.ron` (`TerrainData::floors` and friends).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BakedBuilding {
    /// 1-based; the value of its cells in `BakedMap::building_id`.
    pub id: u32,
    /// Id of the authoring footprint feature.
    pub feature_id: u32,
    pub class: BuildingClass,
    pub height_m: f32,
    pub centroid: [f32; 2],
    /// Footprint area (one floor).
    pub footprint_m2: f32,
    /// Raster cells it owns; 0 for a footprint smaller than a cell (it then blocks nothing and
    /// cannot be entered).
    pub cells: u32,
    /// Entrances: midpoints of walls with dry, open ground outside and one of the building's own
    /// cells just inside. A squad enters the building only through these (foot raster).
    pub entrances: Vec<[f32; 2]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BakedMap {
    pub manifest: MapManifest,
    pub input_hash: u64,
    pub elevation: Heightfield,
    pub landcover: Grid<u8>,
    pub soil: Grid<u8>,
    /// Water depth (m), quantised to centimetres.
    pub water_depth: Grid<f32>,
    /// Building height above ground (m, decimetre steps); 0 = no building.
    pub building_height: Grid<f32>,
    /// Building owning each cell (`BakedBuilding::id`); 0 = none.
    pub building_id: Grid<u32>,
    pub buildings: Vec<BakedBuilding>,
    pub mobility: Vec<MobilityLayer>,
    pub contours: Vec<ContourLine>,
    pub features: Features,
}

impl BakedMap {
    pub fn mobility(&self, class_id: &str) -> Option<&MobilityLayer> {
        self.mobility.iter().find(|m| m.class.id == class_id)
    }

    /// Building at a point, if any.
    pub fn building_at(&self, x: f64, y: f64) -> Option<&BakedBuilding> {
        let (i, j) = self.elevation.cell_of(x, y);
        self.building(self.building_id.get(i, j))
    }

    pub fn building(&self, id: u32) -> Option<&BakedBuilding> {
        (id as usize).checked_sub(1).and_then(|k| self.buildings.get(k))
    }

    /// A building squads can enter at `at`, else the one whose centroid is nearest within
    /// `radius` m (a plan checkpoint placed beside a house).
    pub fn enterable_building_near(&self, at: Vec2, radius: f64) -> Option<&BakedBuilding> {
        if let Some(b) = self.building_at(at[0], at[1]).filter(|b| !b.entrances.is_empty()) {
            return Some(b);
        }
        let d = |b: &BakedBuilding| libm::hypot(b.centroid[0] as f64 - at[0], b.centroid[1] as f64 - at[1]);
        self.buildings.iter().filter(|b| !b.entrances.is_empty() && d(b) <= radius).min_by(|a, b| d(a).total_cmp(&d(b)))
    }

    /// Where a foot route into a building should end: the centre of the building's cell that is
    /// open on the foot raster (interior or entrance) nearest its centroid. `None` if the map has
    /// no foot raster or the building cannot be entered.
    pub fn building_goal(&self, id: u32) -> Option<Vec2> {
        let b = self.building(id)?;
        let foot = self.mobility(mobility::FOOT_CLASS)?;
        let cell = self.manifest.cell_size_m;
        let (ci, cj) = self.elevation.cell_of(b.centroid[0] as f64, b.centroid[1] as f64);
        let r = (libm::sqrt(b.footprint_m2 as f64) / cell) as i64 + 3;
        let mut best: Option<(i64, Vec2)> = None;
        for dj in -r..=r {
            for di in -r..=r {
                let (i, j) = (ci as i64 + di, cj as i64 + dj);
                if !self.building_id.in_bounds(i, j) {
                    continue;
                }
                let (i, j) = (i as usize, j as usize);
                if self.building_id.get(i, j) == id && foot.speed.get(i, j) > 0 {
                    let d = di * di + dj * dj;
                    if best.is_none_or(|b| d < b.0) {
                        best = Some((d, [(i as f64 + 0.5) * cell, (j as f64 + 0.5) * cell]));
                    }
                }
            }
        }
        best.map(|b| b.1)
    }

    /// Hash of all baked output (determinism check).
    pub fn output_hash(&self) -> u64 {
        let mut h = StableHasher::new();
        h.u64(self.input_hash);
        for &v in &self.elevation.z.data {
            h.f32(v);
        }
        h.write(&self.landcover.data);
        h.write(&self.soil.data);
        for &v in &self.water_depth.data {
            h.f32(v);
        }
        for &v in &self.building_height.data {
            h.f32(v);
        }
        for &v in &self.building_id.data {
            h.u32(v);
        }
        for b in &self.buildings {
            h.u32(b.id);
            h.u32(b.feature_id);
            h.u8(b.class as u8);
            h.f32(b.height_m);
            h.f32(b.centroid[0]);
            h.f32(b.centroid[1]);
            h.f32(b.footprint_m2);
            h.u32(b.cells);
            h.u32(b.entrances.len() as u32);
            for e in &b.entrances {
                h.f32(e[0]);
                h.f32(e[1]);
            }
        }
        for m in &self.mobility {
            h.str(&m.class.id);
            h.write(&m.speed.data);
        }
        for c in &self.contours {
            h.f32(c.level);
            h.u8(c.kind as u8);
            h.u8(c.closed as u8);
            for p in &c.points {
                h.f32(p[0]);
                h.f32(p[1]);
            }
        }
        h.finish()
    }
}

/// Per-feature nearest-distance scratch over the full grid; reset via the touched list.
struct Nearest {
    d: Vec<f32>,
    s: Vec<f32>,
    touched: Vec<u32>,
}

impl Nearest {
    fn new(n: usize) -> Self {
        Self { d: vec![f32::INFINITY; n], s: vec![0.0; n], touched: Vec::new() }
    }

    fn visit(&mut self, w: usize, i: usize, j: usize, d: f64, s: f64) {
        let k = j * w + i;
        if self.d[k] == f32::INFINITY {
            self.touched.push(k as u32);
        }
        if (d as f32) < self.d[k] {
            self.d[k] = d as f32;
            self.s[k] = s as f32;
        }
    }

    fn drain(&mut self) -> Vec<(usize, f32, f32)> {
        let out = self.touched.iter().map(|&k| (k as usize, self.d[k as usize], self.s[k as usize])).collect();
        for &k in &self.touched {
            self.d[k as usize] = f32::INFINITY;
        }
        self.touched.clear();
        out
    }
}

/// Sample a polyline at `step` metres: (along-distance, point).
fn resample(pts: &[[f64; 2]], step: f64) -> Vec<(f64, [f64; 2])> {
    let mut out = Vec::new();
    let mut s0 = 0.0;
    for k in 0..pts.len().saturating_sub(1) {
        let (a, b) = (pts[k], pts[k + 1]);
        let len = libm::sqrt((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2));
        let n = libm::ceil(len / step).max(1.0) as usize;
        for t in 0..n {
            let f = t as f64 / n as f64;
            out.push((s0 + f * len, [a[0] + f * (b[0] - a[0]), a[1] + f * (b[1] - a[1])]));
        }
        s0 += len;
    }
    if let Some(&last) = pts.last() {
        out.push((s0, last));
    }
    out
}

/// Linear interpolation of a profile sampled at increasing `s`.
fn interp(profile: &[(f64, f64)], s: f64) -> f64 {
    match profile.binary_search_by(|p| p.0.total_cmp(&s)) {
        Ok(i) => profile[i].1,
        Err(0) => profile.first().map_or(0.0, |p| p.1),
        Err(i) if i >= profile.len() => profile.last().map_or(0.0, |p| p.1),
        Err(i) => {
            let (a, b) = (profile[i - 1], profile[i]);
            let t = if b.0 > a.0 { (s - a.0) / (b.0 - a.0) } else { 0.0 };
            a.1 + (b.1 - a.1) * t
        }
    }
}

fn smooth(v: &mut [f64], radius: usize) {
    if v.len() < 3 || radius == 0 {
        return;
    }
    let src = v.to_vec();
    for (i, out) in v.iter_mut().enumerate() {
        let lo = i.saturating_sub(radius);
        let hi = (i + radius + 1).min(src.len());
        *out = src[lo..hi].iter().sum::<f64>() / (hi - lo) as f64;
    }
}

pub fn bake(pkg: &AuthoringPackage, td: &TerrainData, classes: &[MobilityClass]) -> BakedMap {
    let m = &pkg.manifest;
    let cell = m.cell_size_m;
    let (w, h) = (m.width_cells, m.height_cells);
    let mut hf = pkg.edited_elevation();
    let mut landcover = pkg.landcover.clone();
    let mut soil = pkg.soil.clone();
    let mut water = Grid::new(w, h, 0.0f32);
    let mut near = Nearest::new(w * h);
    let lc = |name: &str| td.landcover_id(name).unwrap_or(0);
    let soil_id = |name: &str| td.soils.iter().find(|s| s.name == name).map_or(0, |s| s.id);

    // Forests first so rivers and roads cut through them.
    for f in &pkg.features.forests {
        let id = match f.props.kind {
            ForestKind::Deciduous => lc("deciduous_forest"),
            ForestKind::Coniferous => lc("coniferous_forest"),
        };
        raster::fill_polygon(&f.ring, cell, w, h, |i, j| landcover.set(i, j, id));
    }

    // Rivers: trapezoidal channel below a water surface that never rises downstream.
    let water_lc = lc("water");
    for r in &pkg.features.rivers {
        if r.points.len() < 2 {
            continue;
        }
        let samples = resample(&r.points, cell);
        let mut surf: Vec<f64> = samples.iter().map(|(_, p)| hf.height(p[0], p[1])).collect();
        smooth(&mut surf, 3);
        for k in 1..surf.len() {
            surf[k] = surf[k].min(surf[k - 1]);
        }
        let surface: Vec<(f64, f64)> = samples.iter().zip(&surf).map(|((s, _), &z)| (*s, z)).collect();
        let mut vs = 0.0;
        let mut widths = Vec::new();
        let mut depths = Vec::new();
        for k in 0..r.points.len() {
            if k > 0 {
                let (a, b) = (r.points[k - 1], r.points[k]);
                vs += libm::sqrt((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2));
            }
            widths.push((vs, *r.props.width_m.get(k).unwrap_or(&4.0) as f64));
            depths.push((vs, *r.props.depth_m.get(k).unwrap_or(&1.0) as f64));
        }
        let max_w = widths.iter().map(|p| p.1).fold(0.0, f64::max);
        let max_d = depths.iter().map(|p| p.1).fold(0.0, f64::max);
        let reach = max_w / 2.0 + (max_d * 3.0).max(cell * 2.0);
        raster::near_polyline(&r.points, reach, cell, w, h, |i, j, d, s| near.visit(w, i, j, d, s));
        for (k, d, s) in near.drain() {
            let (d, s) = (d as f64, s as f64);
            let half = interp(&widths, s) / 2.0;
            let depth = interp(&depths, s);
            let z_s = interp(&surface, s);
            let bank = (depth * 3.0).max(cell * 2.0);
            let z_old = hf.z.data[k] as f64;
            let z_new = if d <= half {
                // Flat bed in the inner 60 %, linear rise to the waterline at the edge.
                let t = ((d / half - 0.6) / 0.4).clamp(0.0, 1.0);
                z_s - depth * (1.0 - t)
            } else {
                // Bank: from the waterline back up to the original surface.
                let t = ((d - half) / bank).clamp(0.0, 1.0);
                z_s + (z_old - z_s) * t
            };
            if z_new < z_old {
                hf.z.data[k] = z_new as f32;
            }
            if d <= half {
                let wd = (z_s - hf.z.data[k] as f64).max(0.0) as f32;
                if wd > water.data[k] {
                    water.data[k] = wd;
                }
                if wd > 0.05 {
                    landcover.data[k] = water_lc;
                }
            }
        }
    }

    // Roads: flatten the corridor to a smoothed longitudinal profile (cut and fill).
    for r in &pkg.features.roads {
        if r.points.len() < 2 {
            continue;
        }
        let samples = resample(&r.points, cell);
        let mut prof: Vec<f64> = samples.iter().map(|(_, p)| hf.height(p[0], p[1])).collect();
        // ~60 m moving average, twice: gentle grades, embankments over dips, cuttings through crests.
        let rad = (30.0 / cell).max(1.0) as usize;
        smooth(&mut prof, rad);
        smooth(&mut prof, rad);
        let profile: Vec<(f64, f64)> = samples.iter().zip(&prof).map(|((s, _), &z)| (*s, z)).collect();
        let half = r.props.width_m as f64 / 2.0;
        let shoulder = (cell * 2.0).max(6.0);
        let (road_lc, road_soil) = match r.props.class {
            RoadClass::Main => (lc("road"), Some(soil_id("paved"))),
            RoadClass::Secondary => (lc("road"), Some(soil_id("gravel"))),
            RoadClass::Track => (lc("track"), None),
            RoadClass::Street => (lc("road"), Some(soil_id("paved"))),
        };
        raster::near_polyline(&r.points, half + shoulder, cell, w, h, |i, j, d, s| near.visit(w, i, j, d, s));
        for (k, d, s) in near.drain() {
            let (d, s) = (d as f64, s as f64);
            let target = interp(&profile, s);
            let blend = if d <= half { 1.0 } else { 1.0 - (d - half) / shoulder };
            let z = hf.z.data[k] as f64;
            hf.z.data[k] = (z + (target - z) * blend.clamp(0.0, 1.0)) as f32;
            // Cell centres within half a cell of the road line carry the road, so narrow roads
            // still form a continuous raster corridor.
            if d <= half.max(cell * 0.75) {
                landcover.data[k] = road_lc;
                if let Some(sid) = road_soil {
                    soil.data[k] = sid;
                }
            }
            // Implicit bridge / culvert: the deck spans at least 1.5 cells either side of the
            // centreline so the crossing is never narrower than a vehicle's tracking error.
            if d <= half.max(cell * 1.5) {
                water.data[k] = 0.0;
            }
        }
    }

    // Buildings: footprint cells carry the building height (LOS obstruction, no-go for vehicles)
    // and the id of the building owning them (the tallest where footprints overlap).
    let mut buildings = Grid::new(w, h, 0.0f32);
    let mut building_id = Grid::new(w, h, 0u32);
    for (n, b) in pkg.features.buildings.iter().enumerate() {
        let hgt = (libm::round(b.props.height_m as f64 * 10.0) / 10.0) as f32;
        raster::fill_polygon(&b.ring, cell, w, h, |i, j| {
            let k = j * w + i;
            if building_id.data[k] == 0 || hgt > buildings.data[k] {
                building_id.data[k] = n as u32 + 1;
            }
            buildings.data[k] = buildings.data[k].max(hgt);
        });
    }

    // Quantise so in-memory bakes equal loaded bakes bit-for-bit.
    for v in hf.z.data.iter_mut() {
        *v = (libm::round(*v as f64 / ELEVATION_STEP) * ELEVATION_STEP) as f32;
    }
    for v in water.data.iter_mut() {
        *v = (libm::round(*v as f64 * 100.0) / 100.0) as f32;
    }
    let (building_list, foot_open) = identify_buildings(pkg, &building_id, &water, td.foot.fording_depth_m.get());
    let interior = mobility::quantise(td.foot.building_interior_speed_mps.get());

    let mobility = classes
        .iter()
        .map(|c| {
            let mut speed = mobility::speed_raster(c, td, &hf, &landcover, &water);
            for (k, v) in speed.data.iter_mut().enumerate() {
                if building_id.data[k] != 0 {
                    // Vehicles: no-go (TERRAIN.md §5.4). Foot: walls are no-go, so a squad gets
                    // in only through an entrance cell and then moves at the interior speed.
                    *v = if c.foot && foot_open[k] { interior } else { 0 };
                }
            }
            MobilityLayer { class: c.clone(), speed }
        })
        .collect();
    let contours = contour::extract(&hf, &ContourSpec::default());

    BakedMap {
        manifest: m.clone(),
        input_hash: pkg.input_hash(),
        elevation: hf,
        landcover,
        soil,
        water_depth: water,
        building_height: buildings,
        building_id,
        buildings: building_list,
        mobility,
        contours,
        features: pkg.features.clone(),
    }
}

/// Building list with entrances, and the building cells open to foot movement: interior cells
/// (all four neighbours in the same building) and the inner cell of each entrance.
fn identify_buildings(
    pkg: &AuthoringPackage,
    ids: &Grid<u32>,
    water: &Grid<f32>,
    fording_m: f64,
) -> (Vec<BakedBuilding>, Vec<bool>) {
    let cell = pkg.manifest.cell_size_m;
    let (w, h) = (ids.w, ids.h);
    let mut cells = vec![0u32; pkg.features.buildings.len() + 1];
    let mut open = vec![false; w * h];
    for j in 0..h {
        for i in 0..w {
            let id = ids.get(i, j);
            if id == 0 {
                continue;
            }
            cells[id as usize] += 1;
            let same = |di: i64, dj: i64| {
                ids.in_bounds(i as i64 + di, j as i64 + dj) && ids.get_clamped(i as i64 + di, j as i64 + dj) == id
            };
            open[j * w + i] = same(1, 0) && same(-1, 0) && same(0, 1) && same(0, -1);
        }
    }
    let cell_at = |p: Vec2| -> Option<(usize, usize)> {
        let (i, j) = (libm::floor(p[0] / cell) as i64, libm::floor(p[1] / cell) as i64);
        ids.in_bounds(i, j).then_some((i as usize, j as usize))
    };
    let list = pkg
        .features
        .buildings
        .iter()
        .enumerate()
        .map(|(n, b)| {
            let id = n as u32 + 1;
            let ring = &b.ring;
            // Shoelace area and centroid; twice the signed area gives the winding (CCW > 0).
            let (mut a2, mut cx, mut cy) = (0.0, 0.0, 0.0);
            for k in 0..ring.len() {
                let (p, q) = (ring[k], ring[(k + 1) % ring.len()]);
                let cr = p[0] * q[1] - q[0] * p[1];
                a2 += cr;
                cx += (p[0] + q[0]) * cr;
                cy += (p[1] + q[1]) * cr;
            }
            let centroid = if libm::fabs(a2) > 1e-9 {
                [cx / (3.0 * a2), cy / (3.0 * a2)]
            } else {
                let k = ring.len().max(1) as f64;
                [ring.iter().map(|p| p[0]).sum::<f64>() / k, ring.iter().map(|p| p[1]).sum::<f64>() / k]
            };
            let mut entrances = Vec::new();
            for k in 0..ring.len() {
                let (a, q) = (ring[k], ring[(k + 1) % ring.len()]);
                let (dx, dy) = (q[0] - a[0], q[1] - a[1]);
                let len = libm::sqrt(dx * dx + dy * dy);
                if len < MIN_ENTRANCE_WALL_M {
                    continue;
                }
                let wind = if a2 >= 0.0 { 1.0 } else { -1.0 };
                let nrm = [wind * dy / len, -wind * dx / len];
                let mid = [(a[0] + q[0]) / 2.0, (a[1] + q[1]) / 2.0];
                // The building's own cell just inside the wall, with dry open ground on its
                // outward side (a four-neighbour, so A* can step in without cutting a corner).
                let inner = [0.5, 1.0].iter().find_map(|&f| {
                    let (i, j) = cell_at([mid[0] - nrm[0] * f * cell, mid[1] - nrm[1] * f * cell])?;
                    (ids.get(i, j) == id).then_some((i, j))
                });
                let Some((i, j)) = inner else { continue };
                let outside = [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)].into_iter().any(|(di, dj)| {
                    let (oi, oj) = (i as i64 + di, j as i64 + dj);
                    di as f64 * nrm[0] + dj as f64 * nrm[1] > 0.3
                        && ids.in_bounds(oi, oj)
                        && ids.get(oi as usize, oj as usize) == 0
                        && (water.get(oi as usize, oj as usize) as f64) <= fording_m
                });
                if outside {
                    open[j * w + i] = true;
                    entrances.push([mid[0] as f32, mid[1] as f32]);
                }
            }
            BakedBuilding {
                id,
                feature_id: b.id,
                class: b.props.class,
                height_m: (libm::round(b.props.height_m as f64 * 10.0) / 10.0) as f32,
                centroid: [centroid[0] as f32, centroid[1] as f32],
                footprint_m2: (libm::fabs(a2) / 2.0) as f32,
                cells: cells[id as usize],
                entrances,
            }
        })
        .collect();
    (list, open)
}

/// Shortest wall that gets an entrance: a door and room to reach it.
const MIN_ENTRANCE_WALL_M: f64 = 2.0;

// --- baked package I/O --------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BakedManifest {
    pub map: MapManifest,
    pub baker_version: u32,
    pub input_hash: u64,
    pub output_hash: u64,
    /// Elevation = (offset_steps + stored u16) × ELEVATION_STEP.
    pub elevation_offset_steps: i64,
    pub mobility_classes: Vec<BakedClass>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BakedClass {
    pub id: String,
    pub road_speed: f64,
    pub cross_country_speed: f64,
    pub max_slope: f64,
    pub fording_depth: f64,
    #[serde(default)]
    pub foot: bool,
}

const MAGIC: &[u8; 4] = b"TKL1";

fn write_layer(path: &Path, w: usize, h: usize, bytes: &[u8]) -> Result<()> {
    let mut f = std::fs::File::create(path).map_err(|e| TerrainError::io(path, e))?;
    let mut head = Vec::with_capacity(12);
    head.extend_from_slice(MAGIC);
    head.extend_from_slice(&(w as u32).to_le_bytes());
    head.extend_from_slice(&(h as u32).to_le_bytes());
    f.write_all(&head).map_err(|e| TerrainError::io(path, e))?;
    let z = zstd::encode_all(bytes, 9).map_err(|e| TerrainError::io(path, e))?;
    f.write_all(&z).map_err(|e| TerrainError::io(path, e))
}

fn read_layer(path: &Path) -> Result<(usize, usize, Vec<u8>)> {
    let mut f = std::fs::File::open(path).map_err(|e| TerrainError::io(path, e))?;
    let mut head = [0u8; 12];
    f.read_exact(&mut head).map_err(|e| TerrainError::io(path, e))?;
    if &head[0..4] != MAGIC {
        return Err(TerrainError::io(path, "bad layer magic"));
    }
    let w = u32::from_le_bytes(head[4..8].try_into().unwrap()) as usize;
    let h = u32::from_le_bytes(head[8..12].try_into().unwrap()) as usize;
    let bytes = zstd::decode_all(f).map_err(|e| TerrainError::io(path, e))?;
    Ok((w, h, bytes))
}

fn u8_grid(path: &Path) -> Result<Grid<u8>> {
    let (w, h, b) = read_layer(path)?;
    if b.len() != w * h {
        return Err(TerrainError::io(path, "layer size mismatch"));
    }
    Ok(Grid::from_vec(w, h, b))
}

fn u16_grid(path: &Path) -> Result<Grid<u16>> {
    let (w, h, b) = read_layer(path)?;
    if b.len() != w * h * 2 {
        return Err(TerrainError::io(path, "layer size mismatch"));
    }
    Ok(Grid::from_vec(w, h, b.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect()))
}

fn u32_grid(path: &Path) -> Result<Grid<u32>> {
    let (w, h, b) = read_layer(path)?;
    if b.len() != w * h * 4 {
        return Err(TerrainError::io(path, "layer size mismatch"));
    }
    Ok(Grid::from_vec(w, h, b.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c)).collect()))
}

fn u16_bytes(g: &Grid<u16>) -> Vec<u8> {
    g.data.iter().flat_map(|v| v.to_le_bytes()).collect()
}

impl BakedMap {
    pub fn save(&self, dir: &Path) -> Result<()> {
        let md = dir.join("mobility");
        std::fs::create_dir_all(&md).map_err(|e| TerrainError::io(&md, e))?;
        let (w, h) = (self.elevation.z.w, self.elevation.z.h);
        let (lo, _) = self.elevation.min_max();
        let offset = libm::round(lo as f64 / ELEVATION_STEP) as i64;
        let q =
            self.elevation.z.map(|v| (libm::round(v as f64 / ELEVATION_STEP) as i64 - offset).clamp(0, 65535) as u16);
        write_layer(&dir.join("elevation.bin"), w, h, &u16_bytes(&q))?;
        write_layer(&dir.join("landcover.bin"), w, h, &self.landcover.data)?;
        write_layer(&dir.join("soil.bin"), w, h, &self.soil.data)?;
        let wd = self.water_depth.map(|v| libm::round(v as f64 * 100.0).clamp(0.0, 65535.0) as u16);
        write_layer(&dir.join("water_depth.bin"), w, h, &u16_bytes(&wd))?;
        let bh = self.building_height.map(|v| libm::round(v as f64 * 10.0).clamp(0.0, 65535.0) as u16);
        write_layer(&dir.join("buildings.bin"), w, h, &u16_bytes(&bh))?;
        let ids: Vec<u8> = self.building_id.data.iter().flat_map(|v| v.to_le_bytes()).collect();
        write_layer(&dir.join("building_id.bin"), w, h, &ids)?;
        let list = ron::ser::to_string_pretty(&self.buildings, ron::ser::PrettyConfig::default())
            .map_err(|e| TerrainError::io(dir, e))?;
        let lp = dir.join("buildings.ron");
        std::fs::write(&lp, list).map_err(|e| TerrainError::io(&lp, e))?;
        for m in &self.mobility {
            write_layer(&md.join(format!("{}.bin", m.class.id)), w, h, &m.speed.data)?;
        }
        write_contours(&dir.join("contours.bin"), &self.contours)?;
        self.features.save(&dir.join("features"))?;
        let manifest = BakedManifest {
            map: self.manifest.clone(),
            baker_version: BAKER_VERSION,
            input_hash: self.input_hash,
            output_hash: self.output_hash(),
            elevation_offset_steps: offset,
            mobility_classes: self
                .mobility
                .iter()
                .map(|m| BakedClass {
                    id: m.class.id.clone(),
                    road_speed: m.class.road_speed,
                    cross_country_speed: m.class.cross_country_speed,
                    max_slope: m.class.max_slope,
                    fording_depth: m.class.fording_depth,
                    foot: m.class.foot,
                })
                .collect(),
        };
        let text = ron::ser::to_string_pretty(&manifest, ron::ser::PrettyConfig::default())
            .map_err(|e| TerrainError::io(dir, e))?;
        let mp = dir.join("manifest.ron");
        std::fs::write(&mp, text).map_err(|e| TerrainError::io(&mp, e))
    }

    pub fn load(dir: &Path) -> Result<Self> {
        let mp = dir.join("manifest.ron");
        let text = std::fs::read_to_string(&mp).map_err(|e| TerrainError::io(&mp, e))?;
        let man: BakedManifest = ron::from_str(&text).map_err(|e| TerrainError::io(&mp, e))?;
        if man.baker_version != BAKER_VERSION {
            return Err(TerrainError(format!(
                "{}: baked with baker v{}, this build is v{BAKER_VERSION}; re-bake",
                dir.display(),
                man.baker_version
            )));
        }
        let q = u16_grid(&dir.join("elevation.bin"))?;
        let z = q.map(|v| ((man.elevation_offset_steps + v as i64) as f64 * ELEVATION_STEP) as f32);
        let wd = u16_grid(&dir.join("water_depth.bin"))?.map(|v| (v as f64 / 100.0) as f32);
        let bh = u16_grid(&dir.join("buildings.bin"))?.map(|v| (v as f64 / 10.0) as f32);
        let lp = dir.join("buildings.ron");
        let list = std::fs::read_to_string(&lp).map_err(|e| TerrainError::io(&lp, e))?;
        let buildings: Vec<BakedBuilding> = ron::from_str(&list).map_err(|e| TerrainError::io(&lp, e))?;
        let mut mobility = Vec::new();
        for c in &man.mobility_classes {
            mobility.push(MobilityLayer {
                class: MobilityClass {
                    id: c.id.clone(),
                    road_speed: c.road_speed,
                    cross_country_speed: c.cross_country_speed,
                    max_slope: c.max_slope,
                    fording_depth: c.fording_depth,
                    foot: c.foot,
                },
                speed: u8_grid(&dir.join("mobility").join(format!("{}.bin", c.id)))?,
            });
        }
        let baked = Self {
            elevation: Heightfield::new(z, man.map.cell_size_m),
            landcover: u8_grid(&dir.join("landcover.bin"))?,
            soil: u8_grid(&dir.join("soil.bin"))?,
            water_depth: wd,
            building_height: bh,
            building_id: u32_grid(&dir.join("building_id.bin"))?,
            buildings,
            mobility,
            contours: read_contours(&dir.join("contours.bin"))?,
            features: Features::load(&dir.join("features"))?,
            input_hash: man.input_hash,
            manifest: man.map,
        };
        if baked.output_hash() != man.output_hash {
            return Err(TerrainError(format!("{}: baked package hash mismatch; re-bake", dir.display())));
        }
        Ok(baked)
    }
}

fn write_contours(path: &Path, lines: &[ContourLine]) -> Result<()> {
    let mut b = Vec::new();
    b.extend_from_slice(&(lines.len() as u32).to_le_bytes());
    for l in lines {
        b.extend_from_slice(&l.level.to_le_bytes());
        b.push(l.kind as u8);
        b.push(l.closed as u8);
        b.extend_from_slice(&(l.points.len() as u32).to_le_bytes());
        for p in &l.points {
            b.extend_from_slice(&p[0].to_le_bytes());
            b.extend_from_slice(&p[1].to_le_bytes());
        }
    }
    write_layer(path, lines.len(), 1, &b)
}

fn read_contours(path: &Path) -> Result<Vec<ContourLine>> {
    let (_, _, b) = read_layer(path)?;
    let bad = || TerrainError::io(path, "truncated contour file");
    let mut at = 0usize;
    let mut take = |n: usize| -> Result<&[u8]> {
        let s = b.get(at..at + n).ok_or_else(bad)?;
        at += n;
        Ok(s)
    };
    let f32_of = |s: &[u8]| f32::from_le_bytes(s.try_into().unwrap());
    let u32_of = |s: &[u8]| u32::from_le_bytes(s.try_into().unwrap());
    let n = u32_of(take(4)?) as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let level = f32_of(take(4)?);
        let kind = match take(1)?[0] {
            0 => ContourKind::Index,
            1 => ContourKind::Intermediate,
            _ => ContourKind::Supplementary,
        };
        let closed = take(1)?[0] != 0;
        let np = u32_of(take(4)?) as usize;
        let mut points = Vec::with_capacity(np);
        for _ in 0..np {
            let x = f32_of(take(4)?);
            let y = f32_of(take(4)?);
            points.push([x, y]);
        }
        out.push(ContourLine { level, kind, closed, points });
    }
    Ok(out)
}
