//! Recipes: preset + per-stage parameters (`data/recipes/<preset>.ron`).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recipe {
    pub preset: String,
    pub landform: LandformParams,
    pub erosion: ErosionParams,
    pub rivers: RiverParams,
    pub cover: CoverParams,
    pub roads: RoadParams,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LandformParams {
    /// Peak-to-trough relief of the noise stack (m).
    pub relief_m: f64,
    /// Wavelength of the largest landforms (m).
    pub wavelength_m: f64,
    pub octaves: u32,
    /// 0 = rolling (fBm), 1 = ridged.
    pub ridge_mix: f64,
    /// Domain-warp displacement (m).
    pub warp_m: f64,
    /// Regional tilt across the map (m), sets the dominant drainage direction.
    pub tilt_m: f64,
    pub tilt_azimuth_deg: f64,
    /// Base elevation above datum (m).
    pub base_m: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErosionParams {
    pub iterations: u32,
    /// Stream-power erodibility × time step (Braun & Willett 2013 implicit scheme), n = 1.
    pub k: f64,
    /// Area exponent m.
    pub m: f64,
    /// Thermal erosion talus slope (rise/run).
    pub talus: f64,
    pub thermal_iterations: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiverParams {
    /// Drainage area for a channel to start (m²).
    pub min_area_m2: f64,
    /// Hydraulic geometry (Leopold & Maddock 1953), A in km²: width = a·A^b, depth = c·A^d.
    pub width_coef: f64,
    pub width_exp: f64,
    pub depth_coef: f64,
    pub depth_exp: f64,
    pub min_length_m: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverParams {
    /// Target forest share of the map (0..1).
    pub forest_fraction: f64,
    /// Patchiness wavelength of forest stands (m).
    pub forest_wavelength_m: f64,
    /// Moisture (0–255) above which flat ground becomes marsh.
    pub marsh_moisture: u8,
    /// Typical field parcel size (m).
    pub field_size_m: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoadParams {
    /// Mean spacing between villages (m).
    pub village_spacing_m: f64,
    pub village_radius_m: f64,
    /// Number of roads leaving the map edge.
    pub edge_exits: u32,
    /// One market town near the map centre (M8h1b); none when absent.
    #[serde(default)]
    pub town: Option<TownParams>,
}

/// Town layout (`terragen::town`; plan M8H1B-TOWNS §2). Generator tuning, not sim data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TownParams {
    /// Outer radius of the built area (m).
    pub radius_m: f64,
    /// Radius of the old core: closed blocks, bent streets (m).
    pub core_radius_m: f64,
    /// Street lattice spacing: one core block (m). Residential blocks are two, high-rise and
    /// industrial blocks four.
    pub block_m: f64,
    /// Street node jitter in the core (m).
    pub core_jitter_m: f64,
    /// Street widths in the core and outside it (m).
    pub core_street_m: f64,
    pub street_m: f64,
    /// Angular width of the high-rise estate and of the industrial sector opposite it (deg).
    pub highrise_sector_deg: f64,
    pub industrial_sector_deg: f64,
}

impl Recipe {
    pub fn from_ron(text: &str) -> Result<Self, String> {
        ron::from_str(text).map_err(|e| e.to_string())
    }
}
