//! Fire support (DESIGN.md §6.5, decision 21): batteries called by the player, their effects on
//! vehicles, and suppression. `data/combat/fires.ron`.

use serde::{Deserialize, Serialize};

use crate::{Outcomes, ServiceDates, Stat, VehicleClass};

/// A self-propelled battery type. A scenario places one (`BatteryAssign`) for a side.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryDef {
    pub id: String,
    pub name: String,
    /// The real design it mirrors (documentation only).
    pub mirrors: String,
    pub in_service: ServiceDates,
    /// Guns that fire together in one volley.
    pub tubes: u32,
    pub min_range_m: Stat<f64>,
    pub max_range_m: Stat<f64>,
    /// Time from the call for fire to "shot" (s).
    pub fdc_delay_s: Stat<f64>,
    /// Time between volleys of a fire-for-effect mission (s).
    pub volley_interval_s: Stat<f64>,
    /// Time of flight: `base + per_km · range` (s).
    pub tof_base_s: Stat<f64>,
    pub tof_per_km_s: Stat<f64>,
    /// 1σ range error as a fraction of range.
    pub range_error_frac: Stat<f64>,
    /// 1σ deflection error (mrad).
    pub deflection_error_mrad: Stat<f64>,
    pub he_rounds: u32,
    pub smoke_rounds: u32,
    /// Smoke kind id in `smoke.ron` for a smoke round.
    pub smoke_kind: String,
}

/// Effect of one HE shell on one vehicle class.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassEffect {
    pub class: VehicleClass,
    /// Within `direct_radius_m`: hit on the roof, tracks or running gear.
    pub direct: Outcomes,
    /// Within `near_radius_m`: blast and fragments.
    pub near: Outcomes,
    /// Chance that fragments damage the sights.
    pub near_sights: Stat<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeEffects {
    pub direct_radius_m: Stat<f64>,
    pub near_radius_m: Stat<f64>,
    pub classes: Vec<ClassEffect>,
}

/// Suppression: a 0..1 state per vehicle that rises under fire and decays.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuppressionModel {
    /// A burst this close adds suppression, falling linearly to zero at the radius (m).
    pub burst_radius_m: Stat<f64>,
    /// Suppression added by a burst at the vehicle (0..1).
    pub burst_peak: Stat<f64>,
    pub decay_per_s: Stat<f64>,
    /// Added to the target of a direct-fire round that misses.
    pub near_miss: Stat<f64>,
    /// Added to the target of a round that hits but does not penetrate.
    pub glancing_hit: Stat<f64>,
    /// Detection cycles are multiplied by `1 - sensor_penalty · s` (crews buttoned up).
    pub sensor_penalty: Stat<f64>,
    /// Laying and reload times are multiplied by `1 + slow_gain · s`.
    pub slow_gain: Stat<f64>,
    /// Platoon drill "react to indirect fire": a burst within this distance of a member (m)...
    pub react_radius_m: Stat<f64>,
    /// ...in the last this many seconds triggers it.
    pub react_window_s: Stat<f64>,
    /// Minimum time between reports (s).
    pub react_cooldown_s: Stat<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FireSupportModel {
    pub batteries: Vec<BatteryDef>,
    pub effects: HeEffects,
    pub suppression: SuppressionModel,
}

impl FireSupportModel {
    pub fn battery(&self, id: &str) -> Option<&BatteryDef> {
        self.batteries.iter().find(|b| b.id == id)
    }

    pub fn class_effect(&self, class: VehicleClass) -> Option<&ClassEffect> {
        self.effects.classes.iter().find(|c| c.class == class)
    }
}
