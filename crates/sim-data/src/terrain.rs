use serde::{Deserialize, Serialize};

use crate::Stat;

/// Land-cover class table (`data/terrain/landcover.ron`). Ids are stable u8 raster values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LandCoverDef {
    pub id: u8,
    pub name: String,
    /// Display fill colour (RGB). Desaturated; the renderer may further modulate it.
    pub display_rgb: [u8; 3],
    /// Multiplier on achievable cross-country speed from vegetation/obstacles (0 = no-go).
    pub speed_factor: Stat<f64>,
    /// Micro-relief amplitude used by `h_micro` (m).
    pub micro_relief_m: Stat<f64>,
    #[serde(default)]
    pub is_forest: bool,
    #[serde(default)]
    pub is_water: bool,
}

/// Soil class table (`data/terrain/soils.ron`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoilDef {
    pub id: u8,
    pub name: String,
    /// Tracked-vehicle rolling resistance coefficient (dimensionless).
    pub rolling_resistance: Stat<f64>,
    /// Traction coefficient: max tractive force / weight.
    pub traction_coefficient: Stat<f64>,
}

#[derive(Debug, Clone)]
pub struct TerrainData {
    pub landcover: Vec<LandCoverDef>,
    pub soils: Vec<SoilDef>,
}

impl TerrainData {
    pub fn validate(&self) -> Result<(), String> {
        for (i, lc) in self.landcover.iter().enumerate() {
            if lc.id as usize != i {
                return Err(format!("landcover ids must be 0..n in order; '{}' has id {}", lc.name, lc.id));
            }
        }
        for (i, s) in self.soils.iter().enumerate() {
            if s.id as usize != i {
                return Err(format!("soil ids must be 0..n in order; '{}' has id {}", s.name, s.id));
            }
        }
        Ok(())
    }

    pub fn landcover(&self, id: u8) -> &LandCoverDef {
        &self.landcover[(id as usize).min(self.landcover.len() - 1)]
    }

    pub fn soil(&self, id: u8) -> &SoilDef {
        &self.soils[(id as usize).min(self.soils.len() - 1)]
    }

    pub fn landcover_id(&self, name: &str) -> Option<u8> {
        self.landcover.iter().find(|l| l.name == name).map(|l| l.id)
    }
}
