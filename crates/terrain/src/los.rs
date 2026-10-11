//! Line of sight (DESIGN.md §5.3, TERRAIN.md §6.1).
//!
//! - Point-to-point: march the ray over the ground at half-cell steps, comparing the sight line
//!   with ground + micro-relief (micro-relief only within `MICRO_RANGE` of either end, where
//!   it decides hull-down; beyond that macro relief suffices). Earth curvature with standard
//!   refraction (k = 0.13) is included.
//! - Partial visibility: several target sample heights are tested in one march, giving the
//!   visible fraction (hull-down detection, hit model).
//! - Vegetation: Beer–Lambert `T = exp(-Σ k·Δs)` along the path inside the canopy volume.
//! - Viewshed: radial sweep keeping the maximum elevation angle (after Franklin & Ray 1994).

use crate::Grid;
use crate::Vec2;
use crate::ground::Ground;

const EARTH_R: f64 = 6_371_000.0;
const REFRACTION_K: f64 = 0.13;
/// Micro-relief is evaluated within this distance of the observer and the target (m).
const MICRO_RANGE: f64 = 150.0;
/// Samples this close to either end are ignored (the vehicles' own footprints) (m).
const END_CLEARANCE: f64 = 5.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Sight {
    /// One flag per requested target height.
    pub visible: Vec<bool>,
    /// Vegetation transmittance along the middle sight line (1 = clear).
    pub transmittance: f64,
}

impl Sight {
    pub fn fraction(&self) -> f64 {
        if self.visible.is_empty() {
            return 0.0;
        }
        self.visible.iter().filter(|&&v| v).count() as f64 / self.visible.len() as f64
    }

    pub fn any(&self) -> bool {
        self.visible.iter().any(|&v| v)
    }
}

/// Sight from an eye `eye_agl` above the ground at `from` to points `heights` above the ground
/// at `to`.
pub fn sight(g: &Ground, from: Vec2, eye_agl: f64, to: Vec2, heights: &[f64]) -> Sight {
    let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
    let d = libm::sqrt(dx * dx + dy * dy);
    let z0 = g.height(from[0], from[1]) + eye_agl;
    let zg1 = g.height(to[0], to[1]);
    let mut visible = vec![true; heights.len()];
    let mid = heights.len() / 2;
    let mut optical = 0.0;
    if d < 2.0 * END_CLEARANCE {
        return Sight { visible, transmittance: 1.0 };
    }
    // Half-cell steps near the ends (micro-relief decides hull-down there), full cells between.
    let fine = g.map.elevation.cell * 0.5;
    let coarse = g.map.elevation.cell;
    let mut s = 0.0;
    loop {
        let near_end = s < MICRO_RANGE || s > d - MICRO_RANGE;
        let ds = if near_end { fine } else { coarse };
        s += ds;
        if s >= d - END_CLEARANCE {
            break;
        }
        if s < END_CLEARANCE {
            continue;
        }
        let t = s / d;
        let (x, y) = (from[0] + dx * t, from[1] + dy * t);
        let near = s < MICRO_RANGE || s > d - MICRO_RANGE;
        let bulge = s * (d - s) * (1.0 - REFRACTION_K) / (2.0 * EARTH_R);
        let ground = if near { g.height(x, y) } else { g.macro_height(x, y) } + bulge;
        let mut all_blocked = true;
        for (h, vis) in heights.iter().zip(visible.iter_mut()) {
            if !*vis {
                continue;
            }
            let ray = z0 + (zg1 + h - z0) * t;
            if ground > ray {
                *vis = false;
            } else {
                all_blocked = false;
            }
        }
        if all_blocked {
            return Sight { visible, transmittance: 0.0 };
        }
        let (canopy, kk) = g.canopy(x, y);
        if canopy > 0.0 && kk > 0.0 {
            let ray = z0 + (zg1 + heights.get(mid).copied().unwrap_or(0.0) - z0) * t;
            if ray < ground + canopy {
                optical += kk * ds;
            }
        }
    }
    Sight { visible, transmittance: libm::exp(-optical) }
}

/// Visibility map around an observer: cell value 0 = hidden, 255 = visible (target point
/// `target_agl` above ground), intermediate = seen through vegetation (transmittance × 255).
/// Grid covers `[ox - r, ox + r]²` at `step` metres; returns (grid, origin x, origin y).
pub fn viewshed(g: &Ground, obs: Vec2, eye_agl: f64, target_agl: f64, radius: f64, step: f64) -> (Grid<u8>, f64, f64) {
    let n = (2.0 * radius / step).ceil() as usize + 1;
    let (ox, oy) = (obs[0] - radius, obs[1] - radius);
    let mut out = Grid::new(n, n, 0u8);
    let z0 = g.height(obs[0], obs[1]) + eye_agl;
    let rays = ((std::f64::consts::TAU * radius / step).ceil() as usize).max(8);
    let samples = (radius / (step * 0.5)).ceil() as usize;
    let size = (g.map.manifest.width_m(), g.map.manifest.height_m());
    for r in 0..rays {
        let a = r as f64 / rays as f64 * std::f64::consts::TAU;
        let (sa, ca) = (libm::sin(a), libm::cos(a));
        let mut max_slope = f64::NEG_INFINITY;
        let mut optical = 0.0;
        for k in 1..=samples {
            let s = k as f64 * step * 0.5;
            let (x, y) = (obs[0] + sa * s, obs[1] + ca * s);
            if x < 0.0 || y < 0.0 || x >= size.0 || y >= size.1 {
                break;
            }
            let bulge = s * s * (1.0 - REFRACTION_K) / (2.0 * EARTH_R);
            let ground = if s < MICRO_RANGE { g.height(x, y) } else { g.macro_height(x, y) } - bulge;
            let target_slope = (ground + target_agl - z0) / s;
            let (canopy, kk) = g.canopy(x, y);
            let vis = target_slope >= max_slope && optical < 4.0;
            let (i, j) = (((x - ox) / step) as usize, ((y - oy) / step) as usize);
            if i < n && j < n {
                let v = if vis { (libm::exp(-optical) * 255.0) as u8 } else { 0 };
                let cur = out.get(i, j);
                out.set(i, j, cur.max(v));
            }
            // Obstruction for points beyond: ground, or canopy top once deep in vegetation.
            let obstruct = ground + if canopy > 0.0 { canopy } else { 0.0 };
            max_slope = max_slope.max((obstruct - z0) / s);
            if canopy > 0.0 {
                optical += kk * step * 0.5;
            }
        }
    }
    (out, ox, oy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AuthoringPackage, BakedMap, MapManifest, bake};
    use sim_data::TerrainData;
    use std::path::PathBuf;

    fn td() -> TerrainData {
        sim_data::DataSet::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")).unwrap().terrain
    }

    /// Flat grassland (micro-relief removed by using "road" cover) with an optional ridge.
    fn map(td: &TerrainData, ridge: Option<(f64, f64)>, forest: Option<(f64, f64)>) -> BakedMap {
        let m = MapManifest {
            id: "t".into(),
            name: "t".into(),
            width_cells: 200,
            height_cells: 1000,
            cell_size_m: 4.0,
            seed: 3,
            recipe: None,
        };
        let mut p = AuthoringPackage::new_flat(m, 100.0);
        p.landcover.data.fill(td.landcover_id("road").unwrap());
        for j in 0..1000 {
            let y = (j as f64 + 0.5) * 4.0;
            for i in 0..200 {
                if let Some((y0, h)) = ridge {
                    let z = 100.0 + h * libm::exp(-((y - y0) / 40.0).powi(2));
                    p.elevation_base.set(i, j, z as f32);
                }
                if let Some((a, b)) = forest
                    && y >= a
                    && y < b
                {
                    p.landcover.set(i, j, td.landcover_id("deciduous_forest").unwrap());
                }
            }
        }
        bake(&p, td, &[])
    }

    #[test]
    fn flat_ground_is_visible_and_ridge_blocks() {
        let td = td();
        let flat = map(&td, None, None);
        let g = Ground::new(&flat, &td);
        let s = sight(&g, [400.0, 100.0], 2.3, [400.0, 3500.0], &[0.5, 1.5, 2.3]);
        assert!(s.visible.iter().all(|&v| v) && s.transmittance == 1.0);
        let ridged = map(&td, Some((1800.0, 10.0)), None);
        let g = Ground::new(&ridged, &td);
        let s = sight(&g, [400.0, 100.0], 2.3, [400.0, 3500.0], &[0.5, 1.5, 2.3]);
        assert!(!s.any(), "10 m ridge between two tanks blocks sight");
    }

    #[test]
    fn hull_down_behind_a_crest_shows_only_the_turret() {
        // Target just behind a 1.8 m fold: hull (≤ 1.5 m) hidden, turret (2.3 m) visible from
        // an observer at the same base height 2 km away.
        let td = td();
        let m = map(&td, Some((3000.0, 1.8)), None);
        let g = Ground::new(&m, &td);
        let s = sight(&g, [400.0, 1000.0], 2.3, [400.0, 3060.0], &[0.5, 1.5, 2.3]);
        assert_eq!(s.visible, vec![false, false, true], "{s:?}");
    }

    #[test]
    fn forest_attenuates_by_path_length() {
        let td = td();
        let thick = map(&td, None, Some((1000.0, 1200.0)));
        let thin = map(&td, None, Some((1000.0, 1020.0)));
        let s1 = sight(&Ground::new(&thick, &td), [400.0, 100.0], 2.3, [400.0, 2000.0], &[1.0, 2.0]);
        let s2 = sight(&Ground::new(&thin, &td), [400.0, 100.0], 2.3, [400.0, 2000.0], &[1.0, 2.0]);
        // T = exp(-k·L): k = 0.035/m (data) → 200 m ≈ 0.0009, 20 m ≈ 0.50.
        assert!(s1.transmittance < 0.01, "{}", s1.transmittance);
        assert!((s2.transmittance - (-0.035f64 * 20.0).exp()).abs() < 0.1, "{}", s2.transmittance);
    }

    #[test]
    fn sight_is_symmetric_at_equal_heights() {
        // Property: with equal eye and target heights, A sees B iff B sees A.
        let td = td();
        let m = map(&td, Some((2000.0, 6.0)), None);
        let g = Ground::new(&m, &td);
        for k in 0..200u64 {
            let r = |s: u64| crate::noise::hash01(k as i64, s as i64, 77);
            let a = [50.0 + r(1) * 700.0, 50.0 + r(2) * 3900.0];
            let b = [50.0 + r(3) * 700.0, 50.0 + r(4) * 3900.0];
            let ab = sight(&g, a, 2.0, b, &[2.0]).any();
            let ba = sight(&g, b, 2.0, a, &[2.0]).any();
            assert_eq!(ab, ba, "asymmetric between {a:?} and {b:?}");
        }
    }

    #[test]
    fn viewshed_shadows_behind_ridge() {
        let td = td();
        let m = map(&td, Some((1500.0, 12.0)), None);
        let g = Ground::new(&m, &td);
        let (vs, ox, oy) = viewshed(&g, [400.0, 1000.0], 2.3, 2.3, 1000.0, 8.0);
        let at = |x: f64, y: f64| vs.get(((x - ox) / 8.0) as usize, ((y - oy) / 8.0) as usize);
        assert_eq!(at(400.0, 1300.0), 255, "in front of the ridge");
        assert_eq!(at(400.0, 1700.0), 0, "dead ground behind the ridge");
    }
}
