//! Combat model tables: detection constants and damage outcomes.

use serde::{Deserialize, Serialize};

use crate::Stat;

/// Johnson / TTP and ACQUIRE constants (`data/combat/detection.ron`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionModel {
    pub n50_detect: Stat<f64>,
    pub n50_recognise: Stat<f64>,
    pub n50_identify: Stat<f64>,
    /// ACQUIRE mean search time constant: rate = P∞ / tau (s).
    pub acquire_tau_s: Stat<f64>,
    pub moving_target_factor: Stat<f64>,
    pub firing_target_factor: Stat<f64>,
    pub observer_moving_factor: Stat<f64>,
    /// Optical sight effectiveness multipliers by light level.
    pub light_day: Stat<f64>,
    pub light_twilight: Stat<f64>,
    pub light_night: Stat<f64>,
    /// Maximum range at which detection checks are made (m).
    pub max_range_m: Stat<f64>,
    /// A local track is dropped after this long without sight (s).
    pub track_timeout_s: Stat<f64>,
}

/// Outcome probabilities for one hit category. Remaining probability = no further effect.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Outcomes {
    pub catastrophic: Stat<f64>,
    pub firepower: Stat<f64>,
    pub mobility: Stat<f64>,
    pub crew: Stat<f64>,
}

/// `data/combat/damage.ron`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DamageTables {
    pub turret_penetration: Outcomes,
    pub hull_front_penetration: Outcomes,
    pub hull_side_rear_penetration: Outcomes,
    pub non_penetration: Outcomes,
    /// Multiplier on catastrophic-kill probability for exposed ammunition stowage.
    pub exposed_stowage_factor: Stat<f64>,
    /// Probability of a non-penetrating hit disabling sights.
    pub non_penetration_sights: Stat<f64>,
}

/// Long-rod penetration model (Lanz–Odermatt functional form), `data/combat/penetration.ron`:
/// `P/L = a · coth(b0 + b1·L/D) · sqrt(ρp/ρt) · exp(−c·BHN² / (ρp·v²))`
/// with ρ in g/cm³, v in m/s, BHN of the RHA target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PenetrationModel {
    pub a: Stat<f64>,
    pub b0: Stat<f64>,
    pub b1: Stat<f64>,
    pub c: Stat<f64>,
    pub target_bhn: Stat<f64>,
    pub target_density_g_cm3: Stat<f64>,
}
