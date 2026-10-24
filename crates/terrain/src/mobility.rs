//! Mobility classes and per-class speed rasters used for route planning (TERRAIN.md §6.2).
//!
//! The baked raster is the *planning* speed: terrain cap (land cover, roads, water) times a slope
//! penalty. The simulation computes actual speed from its own power/traction model and only uses
//! the terrain cap from here.

use sim_data::{DataSet, FootMobilityDef, TerrainData, VehicleDef};

use crate::{Grid, Heightfield};

/// Mobility-relevant subset of a vehicle definition, or the foot class of dismounted infantry.
#[derive(Debug, Clone, PartialEq)]
pub struct MobilityClass {
    pub id: String,
    pub road_speed: f64,
    pub cross_country_speed: f64,
    pub max_slope: f64,
    pub fording_depth: f64,
    /// Foot movement: land cover uses `foot_speed_factor`, and buildings can be entered through
    /// their entrances (baker) instead of being no-go.
    pub foot: bool,
}

/// Id of the foot class's speed raster in the baked map.
pub const FOOT_CLASS: &str = "foot";

impl MobilityClass {
    pub fn foot(f: &FootMobilityDef) -> Self {
        Self {
            id: FOOT_CLASS.to_owned(),
            road_speed: f.road_speed_mps.get(),
            cross_country_speed: f.cross_country_speed_mps.get(),
            max_slope: f.max_slope.get(),
            fording_depth: f.fording_depth_m.get(),
            foot: true,
        }
    }
}

/// Every class the baker writes a planning raster for: one per vehicle, then foot.
pub fn all_classes(ds: &DataSet) -> Vec<MobilityClass> {
    ds.vehicles.iter().map(MobilityClass::from).chain(std::iter::once(MobilityClass::foot(&ds.terrain.foot))).collect()
}

impl From<&VehicleDef> for MobilityClass {
    fn from(v: &VehicleDef) -> Self {
        let m = &v.mobility;
        Self {
            id: v.id.clone(),
            road_speed: m.max_speed_road_mps.get(),
            cross_country_speed: m.max_speed_cross_country_mps.get(),
            max_slope: m.max_slope.get(),
            fording_depth: m.fording_depth_m.get(),
            foot: false,
        }
    }
}

/// Planning-speed rasters are quantised to 0.1 m/s in a u8.
pub const SPEED_QUANTUM: f64 = 0.1;

/// Terrain speed cap (m/s) for a cell, ignoring slope: roads, tracks, vegetation, water.
pub fn terrain_cap(class: &MobilityClass, td: &TerrainData, landcover: u8, water_depth: f32) -> f64 {
    if water_depth as f64 > class.fording_depth {
        return 0.0;
    }
    let lc = td.landcover(landcover);
    match lc.name.as_str() {
        "road" => class.road_speed,
        "track" => (class.cross_country_speed * 1.15).min(class.road_speed),
        _ if class.foot => class.cross_country_speed * lc.foot_speed_factor.get(),
        _ => class.cross_country_speed * lc.speed_factor.get(),
    }
}

/// Planning slope penalty: 1 on the flat, falling quadratically to 0 at the class's max slope.
pub fn slope_factor(class: &MobilityClass, slope: f64) -> f64 {
    if slope >= class.max_slope {
        0.0
    } else {
        let r = slope / class.max_slope;
        (1.0 - r * r).max(0.0)
    }
}

/// Speed (m/s) as a raster value; a passable speed never rounds down to zero.
pub fn quantise(v: f64) -> u8 {
    let q = libm::round(v / SPEED_QUANTUM).clamp(0.0, 255.0) as u8;
    if v > 0.0 { q.max(1) } else { 0 }
}

pub fn speed_raster(
    class: &MobilityClass,
    td: &TerrainData,
    hf: &Heightfield,
    landcover: &Grid<u8>,
    water_depth: &Grid<f32>,
) -> Grid<u8> {
    let mut out = Grid::new(hf.z.w, hf.z.h, 0u8);
    for j in 0..hf.z.h {
        for i in 0..hf.z.w {
            let cap = terrain_cap(class, td, landcover.get(i, j), water_depth.get(i, j));
            out.set(i, j, quantise(cap * slope_factor(class, hf.node_slope(i, j))));
        }
    }
    out
}
