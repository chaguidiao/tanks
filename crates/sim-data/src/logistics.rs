//! Resupply rates (docs/plans/M8C-LOGISTICS.md; `data/combat/logistics.ron`). A supply point
//! transfers fuel, rounds and smoke salvos to the vehicles it serves at these rates, so
//! resupply costs time; the scenario places the points and their stocks.

use serde::{Deserialize, Serialize};

use crate::Stat;

/// How a round is stowed during resupply, which sets its transfer rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StowClass {
    /// Main-gun rounds, handed in and racked one by one (gun-launched missiles too).
    MainGun,
    /// Autocannon rounds, loaded as linked belts.
    Autocannon,
    /// Missiles for an external or turret launcher.
    Missile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogisticsModel {
    pub main_gun_rounds_per_min: Stat<f64>,
    pub autocannon_rounds_per_min: Stat<f64>,
    pub missiles_per_min: Stat<f64>,
    pub smoke_salvos_per_min: Stat<f64>,
    pub fuel_l_per_min: Stat<f64>,
    /// Vehicles a supply point serves at the same time (service-station method).
    pub stations: Stat<f64>,
    /// A vehicle within this distance of the point's centre can be served (m).
    pub point_radius_m: Stat<f64>,
}

impl LogisticsModel {
    pub fn rounds_per_min(&self, class: StowClass) -> f64 {
        match class {
            StowClass::MainGun => self.main_gun_rounds_per_min.get(),
            StowClass::Autocannon => self.autocannon_rounds_per_min.get(),
            StowClass::Missile => self.missiles_per_min.get(),
        }
    }
}
