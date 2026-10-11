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
    pub crew: u8,
    pub sensors: Vec<SensorDef>,
    pub fire_control: FireControlDef,
    pub armament: ArmamentDef,
    pub armour: ArmourDef,
    pub ammo_stowage: AmmoStowage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SensorKind {
    /// Direct-view optics and eyes: degrade with light level.
    Optical,
    /// Active IR / image intensifier: night channel, light-independent but short-ranged.
    NightChannel,
    /// Thermal imager: independent of light.
    Thermal,
}

/// A sight or viewer. Era is data: each subsystem carries its own in-service dates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorDef {
    pub name: String,
    pub kind: SensorKind,
    /// Resolvable cycles per milliradian at the working magnification (Johnson-criteria input).
    pub cycles_per_mrad: Stat<f64>,
    pub in_service: ServiceDates,
    /// Hard range limit (e.g. an active-IR illuminator's reach) (m).
    #[serde(default)]
    pub max_range_m: Option<Stat<f64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FireControlDef {
    pub laser_rangefinder: bool,
    /// 1σ range error with the laser (m).
    pub laser_range_error_m: Stat<f64>,
    /// 1σ range error as a fraction of range without the laser (stadiametric estimate).
    pub estimated_range_error_fraction: Stat<f64>,
    /// 1σ residual ballistic error (charge temperature, wear, computer) (mrad).
    pub ballistic_error_mrad: Stat<f64>,
    /// 1σ aim/tracking error when stationary (mrad).
    pub aim_error_mrad: Stat<f64>,
    /// 1σ stabilisation error when firing on the move (mrad).
    pub moving_error_mrad: Stat<f64>,
    /// Residual lead error as a fraction of the lead angle (mrad scale).
    pub lead_error_fraction: Stat<f64>,
    /// Mean time to lay the gun on a new target after traverse (s).
    pub lay_time_s: Stat<f64>,
    pub lase_time_s: Stat<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmmoLoad {
    pub ammo: String,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArmamentDef {
    pub gun: String,
    pub load: Vec<AmmoLoad>,
    pub autoloader: bool,
    /// Time between rounds once the gun is laid (s).
    pub reload_s: Stat<f64>,
    pub turret_traverse_rad_s: Stat<f64>,
    /// Maximum range the crew will engage at (m).
    pub max_engagement_range_m: Stat<f64>,
}

/// Line-of-sight RHA equivalence against KE at normal attack on each face (mm).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArmourDef {
    pub turret_front: Stat<f64>,
    pub turret_side: Stat<f64>,
    pub turret_rear: Stat<f64>,
    pub hull_front: Stat<f64>,
    pub hull_side: Stat<f64>,
    pub hull_rear: Stat<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AmmoStowage {
    /// Main-gun rounds behind blow-out panels / isolated compartments.
    Isolated,
    /// Rounds in an unprotected carousel/hull: high cook-off risk.
    Exposed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AmmoKind {
    Apfsds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmmoDef {
    pub id: String,
    pub name: String,
    pub mirrors: String,
    pub kind: AmmoKind,
    pub in_service: ServiceDates,
    pub muzzle_velocity_mps: Stat<f64>,
    /// Velocity lost per km of flight (m/s per km); APFSDS loses tens of m/s per km.
    pub velocity_loss_mps_per_km: Stat<f64>,
    pub rod_length_mm: Stat<f64>,
    pub rod_diameter_mm: Stat<f64>,
    pub rod_density_g_cm3: Stat<f64>,
    /// Round-to-round dispersion, 1σ (mrad).
    pub dispersion_mrad: Stat<f64>,
    /// Published/estimated penetration at 2 km into RHA at normal (mm), for validation only.
    #[serde(default)]
    pub reference_penetration_2km_mm: Option<Stat<f64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dimensions {
    /// Hull length (m).
    pub hull_length_m: Stat<f64>,
    pub width_m: Stat<f64>,
    pub height_m: Stat<f64>,
    /// Hull roof height above ground (m): what a hull-down position must hide.
    pub hull_height_m: Stat<f64>,
    /// Gunner's primary sight height above ground (m).
    pub sight_height_m: Stat<f64>,
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
