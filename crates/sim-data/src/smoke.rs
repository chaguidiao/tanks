//! Smoke kinds (`data/combat/smoke.ron`, DESIGN.md §6.1 and decision 18).

use serde::{Deserialize, Serialize};

use crate::{ServiceDates, Stat};

/// One kind of cloud: grenade screen, engine exhaust smoke, shell or artillery smoke. Optical
/// depths are for a path through the cloud's diameter at full strength; a shorter chord
/// scales them down (Beer–Lambert, T = exp(-τ)).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmokeKindDef {
    pub id: String,
    pub name: String,
    pub in_service: ServiceDates,
    /// Radius once fully grown (m).
    pub radius_m: Stat<f64>,
    /// Time to grow from a puff to full size and strength (s).
    pub rise_s: Stat<f64>,
    /// Time until the cloud is gone (s); strength fades over its last half.
    pub life_s: Stat<f64>,
    /// Optical depth across the diameter in the visual and near-IR band (optics, image
    /// intensifiers).
    pub tau_visible: Stat<f64>,
    /// Optical depth across the diameter in the thermal band (8–12 µm).
    pub tau_thermal: Stat<f64>,
    /// How much of the wind velocity the cloud follows (0..1).
    pub drift: Stat<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmokeModel {
    /// A sensor whose path transmittance falls below this sees nothing (0..1).
    pub blind_below: Stat<f64>,
    pub kinds: Vec<SmokeKindDef>,
}

impl SmokeModel {
    pub fn kind(&self, id: &str) -> Option<usize> {
        self.kinds.iter().position(|k| k.id == id)
    }

    /// First of `ids` (listed best first) in service in `year`.
    pub fn pick(&self, ids: &[String], year: i32) -> Option<usize> {
        ids.iter().filter_map(|id| self.kind(id)).find(|&i| self.kinds[i].in_service.contains(year))
    }
}

/// Vehicle smoke grenade launchers (self-screening).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmokeLauncherDef {
    /// Smoke kinds it can fire, best first; the first in service at the scenario date is used.
    pub kinds: Vec<String>,
    /// Salvos carried.
    pub salvos: u32,
    /// Clouds placed per salvo, spread across the arc.
    pub clouds_per_salvo: u32,
    /// Distance from the vehicle at which the clouds form (m).
    pub range_m: Stat<f64>,
    /// Width of the arc around the turret heading (deg).
    pub arc_deg: Stat<f64>,
}

/// Engine exhaust smoke (fuel injected into the exhaust).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineSmokeDef {
    pub kinds: Vec<String>,
}
