//! Baker: a pure function `(authoring package, data) -> baked package` (TERRAIN.md §1, §2.3).
//!
//! Steps: edited elevation → carve rivers → cut/fill roads → rasterise forests → quantise
//! elevation → water depth → per-class speed rasters → contours → hashes.
//!
//! Simplifications in this version (see TERRAIN.md §9): layers are stored as single zstd blocks
//! instead of 256² tiles, and a road crossing a river acts as an implicit bridge.

use std::io::{Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};
use sim_data::TerrainData;

use crate::contour::{self, ContourKind, ContourLine, ContourSpec};
use crate::features::{ForestKind, RoadClass};
use crate::hash::StableHasher;
use crate::mobility::{self, MobilityClass};
use crate::raster;
use crate::{AuthoringPackage, Features, Grid, Heightfield, MapManifest, Result, TerrainError};

/// Bump when baker output changes for the same input.
pub const BAKER_VERSION: u32 = 1;

/// Elevation quantisation step (m) in the baked package.
pub const ELEVATION_STEP: f64 = 0.02;

#[derive(Debug, Clone, PartialEq)]
pub struct MobilityLayer {
    pub class: MobilityClass,
    /// Planning speed, units of `mobility::SPEED_QUANTUM` m/s.
    pub speed: Grid<u8>,
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
    pub mobility: Vec<MobilityLayer>,
    pub contours: Vec<ContourLine>,
    pub features: Features,
}

impl BakedMap {
    pub fn mobility(&self, class_id: &str) -> Option<&MobilityLayer> {
        self.mobility.iter().find(|m| m.class.id == class_id)
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

    // Quantise so in-memory bakes equal loaded bakes bit-for-bit.
    for v in hf.z.data.iter_mut() {
        *v = (libm::round(*v as f64 / ELEVATION_STEP) * ELEVATION_STEP) as f32;
    }
    for v in water.data.iter_mut() {
        *v = (libm::round(*v as f64 * 100.0) / 100.0) as f32;
    }

    let mobility = classes
        .iter()
        .map(|c| MobilityLayer { class: c.clone(), speed: mobility::speed_raster(c, td, &hf, &landcover, &water) })
        .collect();
    let contours = contour::extract(&hf, &ContourSpec::default());

    BakedMap {
        manifest: m.clone(),
        input_hash: pkg.input_hash(),
        elevation: hf,
        landcover,
        soil,
        water_depth: water,
        mobility,
        contours,
        features: pkg.features.clone(),
    }
}

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
        let mut mobility = Vec::new();
        for c in &man.mobility_classes {
            mobility.push(MobilityLayer {
                class: MobilityClass {
                    id: c.id.clone(),
                    road_speed: c.road_speed,
                    cross_country_speed: c.cross_country_speed,
                    max_slope: c.max_slope,
                    fording_depth: c.fording_depth,
                },
                speed: u8_grid(&dir.join("mobility").join(format!("{}.bin", c.id)))?,
            });
        }
        let baked = Self {
            elevation: Heightfield::new(z, man.map.cell_size_m),
            landcover: u8_grid(&dir.join("landcover.bin"))?,
            soil: u8_grid(&dir.join("soil.bin"))?,
            water_depth: wd,
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
