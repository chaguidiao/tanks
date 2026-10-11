use serde::{Deserialize, Serialize};

use crate::{ServiceDates, Stat};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VehicleDef {
    pub id: String,
    /// Fictional display name.
    pub name: String,
    /// The real design it mirrors (documentation only).
    pub mirrors: String,
    pub in_service: ServiceDates,
    pub dimensions: Dimensions,
    pub mobility: MobilityDef,
    pub gun: GunLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dimensions {
    /// Hull length (m).
    pub hull_length_m: Stat<f64>,
    pub width_m: Stat<f64>,
    pub height_m: Stat<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobilityDef {
    pub combat_mass_kg: Stat<f64>,
    /// Gross engine power (W).
    pub engine_power_w: Stat<f64>,
    /// Engine-to-sprocket efficiency (0..1).
    pub drivetrain_efficiency: Stat<f64>,
    /// Governed maximum forward speed on a road (m/s).
    pub max_speed_road_mps: Stat<f64>,
    /// Sustained cross-country speed on good open ground (m/s): ride/suspension limit.
    pub max_speed_cross_country_mps: Stat<f64>,
    pub max_speed_reverse_mps: Stat<f64>,
    /// Maximum longitudinal gradient climbable (rise/run).
    pub max_slope: Stat<f64>,
    /// Maximum side slope (rise/run).
    pub max_side_slope: Stat<f64>,
    /// Unprepared fording depth (m).
    pub fording_depth_m: Stat<f64>,
    pub vertical_step_m: Stat<f64>,
    pub trench_width_m: Stat<f64>,
    pub track_ground_contact_m: Stat<f64>,
    pub track_width_m: Stat<f64>,
    /// Hull yaw rate at low speed / pivot (rad/s).
    pub max_yaw_rate_rad_s: Stat<f64>,
    /// Lateral acceleration limit in turns (m/s²).
    pub max_lateral_accel_mps2: Stat<f64>,
    /// Service braking deceleration (m/s²).
    pub braking_decel_mps2: Stat<f64>,
    pub fuel_capacity_l: Stat<f64>,
    pub fuel_idle_lph: Stat<f64>,
    /// Brake-specific fuel consumption at load (L/kWh).
    pub fuel_per_kwh_l: Stat<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GunLimits {
    pub depression_rad: Stat<f64>,
    pub elevation_rad: Stat<f64>,
}
