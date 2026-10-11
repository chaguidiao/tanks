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
    #[serde(default)]
    pub objectives: Vec<ObjectiveDef>,
    #[serde(default)]
    pub victory: Option<VictoryDef>,
}

/// A named terrain objective (circle).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectiveDef {
    pub name: String,
    pub centre: [f64; 2],
    pub radius_m: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VictoryDef {
    /// Scenario length (s).
    pub time_limit_s: f64,
    /// Objective whose holder at the end wins; ties go to the defender.
    #[serde(default)]
    pub objective: Option<String>,
    /// Index of the attacking side.
    pub attacker: usize,
    /// A side that loses this fraction of its tanks loses at once.
    pub destroyed_fraction: f64,
}

/// Commander-level AI plan for a side (DESIGN.md §9.1): tasks per platoon, executed with the
/// side's doctrine pack templates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiPlan {
    pub tasks: Vec<AiTask>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiTask {
    /// Platoon callsigns assigned to the task.
    pub platoons: Vec<String>,
    pub mission: Mission,
    /// Seconds after start before the task begins.
    #[serde(default)]
    pub start_s: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Mission {
    /// Attack along an axis of advance to an objective.
    Attack { axis: Vec<[f64; 2]>, objective: String },
    /// Occupy and hold a battle position.
    Defend { centre: [f64; 2], radius_m: f64, facing_deg: f64 },
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
    /// Commander AI plan (used when the side is not player-controlled).
    #[serde(default)]
    pub ai: Option<AiPlan>,
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
