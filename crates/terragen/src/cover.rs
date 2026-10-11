//! Stage 5 — moisture, soil and land cover (TERRAIN.md §3).
//!
//! - Moisture from the Topographic Wetness Index, TWI = ln(a / tan β) (Beven & Kirkby 1979).
//! - Soil by slope/wetness rules with noise patches.
//! - Land cover by suitability: marsh in wet flats, forest by a patch field biased to steep
//!   ground (thresholded to hit the target share), fields as jittered-Voronoi parcels.

use sim_data::TerrainData;
use terrain::features::River;
use terrain::{Grid, Heightfield, noise};

use crate::hydro::Flow;
use crate::recipe::CoverParams;

pub struct Cover {
    pub landcover: Grid<u8>,
    pub soil: Grid<u8>,
    pub moisture: Grid<u8>,
}

fn id(td: &TerrainData, name: &str) -> u8 {
    td.landcover_id(name).unwrap_or_else(|| panic!("landcover class '{name}' missing from data"))
}

fn soil_id(td: &TerrainData, name: &str) -> u8 {
    td.soils.iter().find(|s| s.name == name).map(|s| s.id).unwrap_or_else(|| panic!("soil '{name}' missing from data"))
}

pub fn generate(
    hf: &Heightfield,
    flow: &Flow,
    _rivers: &[River],
    p: &CoverParams,
    td: &TerrainData,
    seed: u64,
) -> Cover {
    let (w, h) = (hf.z.w, hf.z.h);
    let cell = hf.cell;
    let slope = crate::par_grid(w, h, |i, j| hf.node_slope(i, j) as f32);
    let moisture = crate::par_grid(w, h, |i, j| {
        let k = j * w + i;
        let a = flow.area[k] as f64 / cell;
        let tb = (slope.data[k] as f64).max(0.001);
        let twi = libm::log(a / tb);
        (((twi - 3.0) / 13.0).clamp(0.0, 1.0) * 255.0) as u8
    });
    let (loam, sand, clay, peat, gravel) =
        (soil_id(td, "loam"), soil_id(td, "sand"), soil_id(td, "clay"), soil_id(td, "peat"), soil_id(td, "gravel"));
    let soil = crate::par_grid(w, h, |i, j| {
        let k = j * w + i;
        let (s, m) = (slope.data[k], moisture.data[k]);
        let (x, y) = ((i as f64 + 0.5) * cell, (j as f64 + 0.5) * cell);
        if s > 0.35 {
            gravel
        } else if m > 220 && s < 0.02 {
            peat
        } else if m > 170 && s < 0.05 {
            clay
        } else if m < 120 && noise::fbm(x / 1500.0, y / 1500.0, seed ^ 21, 3, 2.0, 0.5) > 0.35 {
            sand
        } else {
            loam
        }
    });

    // Forest score: patch field plus a preference for steep ground.
    let fw = p.forest_wavelength_m;
    let score = crate::par_grid(w, h, |i, j| {
        let (x, y) = ((i as f64 + 0.5) * cell, (j as f64 + 0.5) * cell);
        let n = 0.7 * noise::fbm(x / fw, y / fw, seed ^ 31, 4, 2.0, 0.5)
            + 0.3 * noise::fbm(x / (fw / 4.0), y / (fw / 4.0), seed ^ 32, 3, 2.0, 0.5);
        (n + slope.get(i, j) as f64 * 2.5) as f32
    });
    let marsh_m = p.marsh_moisture;
    let is_marsh = |k: usize| moisture.data[k] >= marsh_m && slope.data[k] < 0.03;
    let mut sample: Vec<f32> = (0..w * h).step_by(7).filter(|&k| !is_marsh(k)).map(|k| score.data[k]).collect();
    sample.sort_by(f32::total_cmp);
    let thr = if sample.is_empty() || p.forest_fraction <= 0.0 {
        f32::INFINITY
    } else {
        let q = ((1.0 - p.forest_fraction.clamp(0.0, 1.0)) * (sample.len() - 1) as f64) as usize;
        sample[q]
    };

    let (grass, crop, plough, decid, conif, scrub, marsh) = (
        id(td, "grassland"),
        id(td, "cropland"),
        id(td, "ploughed"),
        id(td, "deciduous_forest"),
        id(td, "coniferous_forest"),
        id(td, "scrub"),
        id(td, "marsh"),
    );
    let fs = p.field_size_m;
    let landcover = crate::par_grid(w, h, |i, j| {
        let k = j * w + i;
        let (x, y) = ((i as f64 + 0.5) * cell, (j as f64 + 0.5) * cell);
        if is_marsh(k) {
            return marsh;
        }
        if score.data[k] >= thr {
            let sd = soil.data[k];
            let c = noise::fbm(x / 1200.0, y / 1200.0, seed ^ 41, 2, 2.0, 0.5);
            return if sd == sand || sd == gravel || c > 0.25 { conif } else { decid };
        }
        if slope.data[k] > 0.22 {
            return scrub;
        }
        if moisture.data[k] > 175 {
            return grass; // wet meadow along streams
        }
        // Field parcel: nearest jittered site on an fs-spaced lattice.
        let (gx, gy) = (libm::floor(x / fs) as i64, libm::floor(y / fs) as i64);
        let mut best = (f64::INFINITY, 0i64, 0i64);
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (cx, cy) = (gx + dx, gy + dy);
                let px = (cx as f64 + 0.15 + 0.7 * noise::hash01(cx, cy, seed ^ 51)) * fs;
                let py = (cy as f64 + 0.15 + 0.7 * noise::hash01(cx, cy, seed ^ 52)) * fs * 0.8;
                let d = (x - px).powi(2) + (y - py * 1.25).powi(2);
                if d < best.0 {
                    best = (d, cx, cy);
                }
            }
        }
        let r = noise::hash01(best.1, best.2, seed ^ 53);
        if r < 0.35 {
            grass
        } else if r < 0.75 {
            crop
        } else {
            plough
        }
    });
    Cover { landcover, soil, moisture }
}
