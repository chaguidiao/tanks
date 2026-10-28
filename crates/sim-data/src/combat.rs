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
    /// Size of the dust and smoke cloud of a rocket or missile launch, seen for a few seconds
    /// after it whatever the firer's camouflage (m; plan M8B-AT-TEAMS §4).
    pub launch_cloud_m: Stat<f64>,
    /// Optical sight effectiveness by sun elevation, listed from the highest elevation down;
    /// interpolated linearly in the logarithm of the factor.
    pub light_curve: Vec<LightPoint>,
    /// Full moon multiplies the night-time factor by `1 + moon_gain` (moon assumed up all night).
    pub moon_gain: Stat<f64>,
    pub thermal_crossover: CrossoverDef,
    /// Maximum range at which detection checks are made (m).
    pub max_range_m: Stat<f64>,
    /// Arc a crew or squad searches for new targets (deg; B24 D2, NVESD field-of-regard search):
    /// a sight with a narrower field of view detects at `fov / sector` of its in-view rate.
    pub search_sector_deg: Stat<f64>,
    /// A local track is dropped after this long without sight (s).
    pub track_timeout_s: Stat<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightPoint {
    pub sun_elevation_deg: f64,
    pub factor: Stat<f64>,
}

/// Thermal crossover: around sunrise and sunset the target and its background reach the same
/// temperature and thermal contrast collapses. The thermal factor is
/// `1 - depth · exp(-((elevation - centre) / width)²)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossoverDef {
    pub centre_elevation_deg: Stat<f64>,
    pub width_deg: Stat<f64>,
    pub depth: Stat<f64>,
}

/// One weather condition (`data/combat/weather.ron`); a scenario names one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherPreset {
    pub id: String,
    pub name: String,
    /// Meteorological visibility: the range at which a black object has 2 % contrast
    /// (Koschmieder, extinction = 3.912 / V) (m).
    pub visibility_m: Stat<f64>,
    /// Extinction in the thermal band (8–12 µm) (per m).
    pub thermal_extinction_per_m: Stat<f64>,
    /// Multiplier on optical light by cloud and precipitation (1 = clear sky).
    pub sky_light: Stat<f64>,
    /// Extra spotting cue for a fast-moving vehicle raising dust (multiplies the moving-target
    /// factor; 1 = none, wet ground or dense air).
    pub dust_cue: Stat<f64>,
    /// Multiplier on soil traction (wet soil gives less grip; 1 = dry going).
    pub traction_factor: Stat<f64>,
    /// Multiplier on soil rolling resistance (wet soil sinks tracks deeper; 1 = dry going).
    pub rolling_factor: Stat<f64>,
    /// Display only (mm/h).
    pub precipitation_mm_h: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherTable {
    pub presets: Vec<WeatherPreset>,
}

impl WeatherTable {
    pub fn preset(&self, id: &str) -> Option<&WeatherPreset> {
        self.presets.iter().find(|p| p.id == id)
    }
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
    /// A shot plunging more steeply than this onto the target meets its top armour (deg; plan
    /// M8B-AT-TEAMS §3).
    pub top_hit_above_deg: Stat<f64>,
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
