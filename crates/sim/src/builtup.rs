//! Built-up ground for tank routes (plan M8F1-INFANTRY-AI §3, decision #2): tanks do not enter
//! built-up areas without dismounted infantry; they overwatch from stand-off (FM 3-06.11 (2002)
//! *Combined Arms Operations in Urban Terrain*). Routes of tank-only platoons use a copy of the
//! vehicle's speed raster in which built-up ground is slow, so the planner goes round it where it
//! can and crosses it by the shortest way where it must.

use std::collections::BTreeMap;

use sim_data::{BuiltUpDoctrine, TerrainData};
use terrain::{BakedMap, Grid};

/// Speed rasters that avoid built-up ground, per (side, vehicle id).
pub type AvoidLayers = BTreeMap<(u8, String), Grid<u8>>;

/// Built-up cells: land cover marked `built_up` and every cell within `margin_m` of a building.
pub fn mask(map: &BakedMap, td: &TerrainData, margin_m: f64) -> Grid<bool> {
    let ids = &map.building_id;
    let (w, h) = (ids.w, ids.h);
    let mut m = Grid::new(w, h, false);
    for j in 0..h {
        for i in 0..w {
            if td.landcover(map.landcover.get(i, j)).built_up {
                m.set(i, j, true);
            }
        }
    }
    let r = (margin_m / map.manifest.cell_size_m).ceil() as i64;
    for j in 0..h {
        for i in 0..w {
            if ids.get(i, j) == 0 {
                continue;
            }
            for dj in -r..=r {
                for di in -r..=r {
                    let (x, y) = (i as i64 + di, j as i64 + dj);
                    if di * di + dj * dj <= r * r && x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
                        m.set(x as usize, y as usize, true);
                    }
                }
            }
        }
    }
    m
}

/// `speed` with built-up cells slowed by `factor` (passable cells stay passable).
pub fn slowed(speed: &Grid<u8>, mask: &Grid<bool>, factor: f64) -> Grid<u8> {
    let data = speed
        .data
        .iter()
        .zip(&mask.data)
        .map(|(&v, &b)| if b && v > 0 { ((v as f64 * factor).round() as u8).max(1) } else { v })
        .collect();
    Grid::from_vec(speed.w, speed.h, data)
}

/// The avoiding rasters for every side and every vehicle type it fields.
pub fn layers(map: &BakedMap, td: &TerrainData, sides: &[(u8, &BuiltUpDoctrine, Vec<String>)]) -> AvoidLayers {
    let mut out = AvoidLayers::new();
    for (side, d, vehicles) in sides {
        let m = mask(map, td, d.margin_m.get());
        for v in vehicles {
            if let Some(layer) = map.mobility(v) {
                out.insert((*side, v.clone()), slowed(&layer.speed, &m, d.speed_factor.get()));
            }
        }
    }
    out
}
