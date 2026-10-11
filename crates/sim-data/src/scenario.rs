use serde::{Deserialize, Serialize};

/// A scenario: map, date, sides and starting units. Lives in `data/scenarios/<id>.ron`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioDef {
    pub id: String,
    pub name: String,
    /// Scenario date (year, month, day). Decides which vehicles exist (era is data).
    pub date: (i32, u8, u8),
    /// Local time at start, seconds after midnight.
    pub start_time_s: u32,
    pub map: MapRef,
    pub seed: u64,
    pub sides: Vec<SideDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapRef {
    /// Map id under `data/maps/<id>`.
    pub id: String,
    /// Recipe used to generate the map if it does not exist yet.
    pub recipe: String,
    pub seed: u64,
    pub size_m: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SideDef {
    pub name: String,
    /// Doctrine pack id (`data/doctrine/<pack>`).
    pub doctrine: String,
    /// Optional per-side tech override: pretend the side lives in this year.
    #[serde(default)]
    pub tech_year: Option<i32>,
    pub units: Vec<UnitDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitDef {
    pub callsign: String,
    pub vehicle: String,
    /// Position in local map metres (x east, y north).
    pub position: [f64; 2],
    /// Compass heading, degrees clockwise from north.
    pub heading_deg: f64,
    /// Platoon callsign; units sharing it form a platoon in listed order (vehicle 1 leads).
    #[serde(default)]
    pub platoon: Option<String>,
}
