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
    /// Situation and mission text for the scenario select screen (plain, terse).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub briefing: Option<String>,
    #[serde(default, skip_serializing_if = "WeatherDef::is_default")]
    pub weather: WeatherDef,
    /// Re-arm events: off-map LOGPACs that refill platoons or batteries at a set time (M8c, C4).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub logistics: Vec<LogisticsEventDef>,
}

/// A scheduled off-map resupply (a LOGPAC from the trains, FM 17-15 (1996) / ATP 3-20.15 (2025)
/// sustainment): at `at_s` the side's listed platoons (or battery) are refilled at once. Applied
/// by the sim as a generated command, so replays regenerate it (DESIGN.md §14 #14).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogisticsEventDef {
    /// Seconds after the scenario start.
    pub at_s: f64,
    /// Index of the side.
    pub side: usize,
    /// Platoon callsigns refilled; empty = every platoon of the side. Unused for `Battery`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub platoons: Vec<String>,
    pub kind: LogisticsKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LogisticsKind {
    /// Each vehicle gets `fraction` of its full load of every round type and of its smoke salvos
    /// (whole rounds, rounded down), never more than the full load.
    Rearm { fraction: f64 },
    /// Each vehicle gets `fraction` of its fuel capacity, never more than a full tank.
    Refuel { fraction: f64 },
    /// The side's battery `callsign` receives `he` and `smoke` rounds.
    Battery {
        callsign: String,
        #[serde(default)]
        he: u32,
        #[serde(default)]
        smoke: u32,
    },
}

/// Weather and sky at the scenario (DESIGN.md decision 20). The preset (`data/combat/weather.ron`)
/// sets visibility, precipitation and cloud; the date, local time and latitude fix the sun.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeatherDef {
    /// Direction the wind blows from (deg, compass).
    #[serde(default)]
    pub wind_from_deg: f64,
    #[serde(default)]
    pub wind_speed_mps: f64,
    #[serde(default = "default_preset")]
    pub preset: String,
    /// Latitude of the map centre (deg, north positive).
    #[serde(default = "default_latitude")]
    pub latitude_deg: f64,
    /// Moon illumination, 0 = new, 1 = full (assumed above the horizon all night).
    #[serde(default)]
    pub moon_phase: f64,
}

fn default_preset() -> String {
    "clear".into()
}

fn default_latitude() -> f64 {
    50.0
}

impl Default for WeatherDef {
    fn default() -> Self {
        Self {
            wind_from_deg: 0.0,
            wind_speed_mps: 0.0,
            preset: default_preset(),
            latitude_deg: default_latitude(),
            moon_phase: 0.0,
        }
    }
}

impl WeatherDef {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
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
    /// Hold the task at its start position until the side's picture shows a live contact (a
    /// reconnaissance report), but no later than `latest_s` (FM 3-98: the main
    /// body moves once reconnaissance has found the enemy).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub release_on_contact: bool,
    /// Latest release time (s since the scenario start) for `release_on_contact`; `None` = wait
    /// for contact only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_s: Option<f64>,
}

/// A battle position: a circle with the direction of the expected threat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BattlePosition {
    pub centre: [f64; 2],
    pub radius_m: f64,
    pub facing_deg: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Mission {
    /// Attack along an axis of advance to an objective.
    Attack { axis: Vec<[f64; 2]>, objective: String },
    /// Occupy and hold a battle position. `alternates` are the supplementary / alternate
    /// positions the platoons displace to, in order, when the doctrine's trigger is met
    /// (ATP 3-20.15 ch. 5).
    Defend {
        centre: [f64; 2],
        radius_m: f64,
        facing_deg: f64,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        alternates: Vec<BattlePosition>,
    },
    /// Reconnaissance: move ahead of the main body along the axis, stop at the first contact (or
    /// at the end of the axis) and observe from the position (FM 3-98).
    Recon { axis: Vec<[f64; 2]>, observe: BattlePosition },
    /// Hold an assembly position; when the objective is contested or the side has lost too much,
    /// counter-attack onto the objective (ATP 3-90.5 / FM 3-90: the reserve is committed at the
    /// decisive point).
    Reserve { assembly: BattlePosition, objective: String },
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
    /// Batteries available to call for fire (not on the map as vehicles).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fire_support: Vec<BatteryAssign>,
    /// Supply points the side's platoons can be sent to (M8c, service-station resupply).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supply_points: Vec<SupplyPointDef>,
}

/// A supply point placed for a side: a scenario item with a stock, not an entity
/// (docs/plans/M8C-LOGISTICS.md). Anything not listed is not stocked, unless `unlimited`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplyPointDef {
    pub callsign: String,
    /// Local map metres.
    pub position: [f64; 2],
    /// Stocks everything without limit (training scenarios).
    #[serde(default)]
    pub unlimited: bool,
    #[serde(default)]
    pub fuel_l: f64,
    /// Rounds by ammo id.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rounds: Vec<(String, u32)>,
    #[serde(default)]
    pub smoke_salvos: u32,
}

/// A battery placed for a side (`data/combat/fires.ron`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryAssign {
    pub callsign: String,
    /// Battery type id.
    pub battery: String,
    /// Where the guns are (local map metres); fixes the range to a target.
    pub position: [f64; 2],
    /// Override the type's stock of HE rounds.
    #[serde(default)]
    pub he_rounds: Option<u32>,
    #[serde(default)]
    pub smoke_rounds: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitDef {
    pub callsign: String,
    /// Vehicle type id; empty for a squad.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub vehicle: String,
    /// Squad type id (`data/infantry`); the unit is then a squad, not a vehicle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub squad: Option<String>,
    /// Callsign of the carrier a squad starts in. Its platoon then defaults to the carrier's
    /// platoon callsign plus `-D` (plan M8B-INFANTRY §6), and `position` is ignored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mounted_in: Option<String>,
    /// Position in local map metres (x east, y north).
    pub position: [f64; 2],
    /// Compass heading, degrees clockwise from north.
    pub heading_deg: f64,
    /// Platoon callsign; units sharing it form a platoon in listed order (vehicle 1 leads).
    #[serde(default)]
    pub platoon: Option<String>,
}
