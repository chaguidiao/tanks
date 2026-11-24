//! Obstacles, mines and breaching kits (docs/plans/M8D-ENGINEERS.md §2–§3;
//! `data/combat/obstacles.ron`). Obstacles are a scenario layer owned by a side
//! (`SideDef::obstacles`); this file holds what each kind does to vehicles and men, the mines a
//! minefield is laid with, and the plough and roller kits that breach it.

use serde::{Deserialize, Serialize};

use crate::{ServiceDates, Stat};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObstacleModel {
    pub kinds: Vec<ObstacleKindDef>,
    pub mines: Vec<MineDef>,
    pub kits: Vec<KitDef>,
    /// Width of a lane opened through an obstacle for vehicles (m).
    pub lane_width_m: Stat<f64>,
    /// A breach starts this far short of the obstacle's near edge and ends this far past its far
    /// edge (m).
    pub lane_overrun_m: Stat<f64>,
    /// Engineers opening a vehicle lane by hand through a minefield (m of lane per minute).
    pub hand_breach_mine_m_per_min: Stat<f64>,
    /// A bypass is taken only if the new route is at most this many times the old one.
    pub bypass_detour_factor: Stat<f64>,
    /// A breaching vehicle or squad stops work while suppressed above this level (0–1).
    pub breach_suppression_stop: Stat<f64>,
}

/// What an obstacle does to a vehicle.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VehicleRule {
    /// Closed to vehicles until a lane is opened.
    NoGo,
    /// Crossed at this fraction of the speed the ground allows.
    Slow(f64),
    /// A gap: crossed if its width is within the vehicle's trench crossing and its depth within
    /// its vertical step, else closed.
    Gap,
    /// Passable; the danger is the mines (`ObstacleDef::mines`).
    Mined,
}

/// An obstacle kind: anti-tank ditch, wire, abatis, dragon's teeth or wall, minefield.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObstacleKindDef {
    pub id: String,
    pub name: String,
    pub vehicle: VehicleRule,
    /// Speed factor on foot (0 = closed to men until a lane is opened).
    pub foot_factor: Stat<f64>,
    /// Default width of the band across its line (m); a minefield's is its depth.
    pub width_m: Stat<f64>,
    /// Default depth (ditches) or height (walls, teeth) (m).
    pub height_m: Stat<f64>,
    /// Seen by an observer with a line of sight at up to this range by day in clear air (m).
    pub detect_range_m: Stat<f64>,
    /// Engineers open a lane through this kind by hand in this time (s); None = they cannot
    /// (ditches; minefields use `hand_breach_mine_m_per_min`).
    #[serde(default)]
    pub hand_breach_s: Option<Stat<f64>>,
    /// A plough or roller opens a lane through it (minefields; wire is crushed anyway).
    #[serde(default)]
    pub kit_breach: bool,
}

/// How a mine is set off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fuze {
    /// Pressure under a track or wheel (track-width attack).
    Pressure,
    /// Tilt rod or magnetic influence anywhere under the hull (full-width attack).
    FullWidth,
}

/// An anti-tank mine type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MineDef {
    pub id: String,
    pub name: String,
    pub fuze: Fuze,
    pub in_service: ServiceDates,
    /// Width of the path through the field in which a tank sets the mine off (m): the expected
    /// strikes crossing a field are linear density × this width.
    pub effective_width_m: Stat<f64>,
    /// Shares of strikes that destroy the vehicle and that immobilise it (the rest do no
    /// lasting harm).
    pub destroy_share: Stat<f64>,
    pub immobilise_share: Stat<f64>,
}

/// What a breaching kit does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KitKind {
    Plough,
    Roller,
}

/// A mine plough or roller fitted to a tank (`UnitDef::kit`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KitDef {
    pub id: String,
    pub name: String,
    pub kind: KitKind,
    pub in_service: ServiceDates,
    /// Vehicle types it fits.
    pub fits: Vec<String>,
    /// Speed while breaching (m/s).
    pub breach_speed_mps: Stat<f64>,
    /// Share of pressure and of full-width mines in its path it clears or sets off harmlessly.
    pub clear_pressure: Stat<f64>,
    pub clear_full_width: Stat<f64>,
    /// Strikes a roller takes before it is wrecked (None for a plough).
    #[serde(default)]
    pub strikes: Option<Stat<f64>>,
}

impl ObstacleModel {
    pub fn kind(&self, id: &str) -> Option<&ObstacleKindDef> {
        self.kinds.iter().find(|k| k.id == id)
    }

    pub fn kind_index(&self, id: &str) -> Option<usize> {
        self.kinds.iter().position(|k| k.id == id)
    }

    pub fn mine(&self, id: &str) -> Option<&MineDef> {
        self.mines.iter().find(|m| m.id == id)
    }

    pub fn mine_index(&self, id: &str) -> Option<usize> {
        self.mines.iter().position(|m| m.id == id)
    }

    pub fn kit(&self, id: &str) -> Option<&KitDef> {
        self.kits.iter().find(|k| k.id == id)
    }

    pub fn kit_index(&self, id: &str) -> Option<usize> {
        self.kits.iter().position(|k| k.id == id)
    }

    pub fn validate(&self) -> Result<(), String> {
        for m in &self.mines {
            let (d, i) = (m.destroy_share.get(), m.immobilise_share.get());
            if d < 0.0 || i < 0.0 || d + i > 1.0 {
                return Err(format!("mine {}: shares must be >= 0 and sum to at most 1", m.id));
            }
        }
        for k in &self.kits {
            for v in [k.clear_pressure.get(), k.clear_full_width.get()] {
                if !(0.0..=1.0).contains(&v) {
                    return Err(format!("kit {}: clearance shares must be in 0..1", k.id));
                }
            }
        }
        Ok(())
    }
}

/// Probability that a vehicle crossing a minefield sets off at least one mine (FM 20-32 (1998)
/// ch. 2–3, probability of encounter): the expected strikes are the sum over mine types of linear
/// density (mines per metre of front) × the type's effective width, and strikes are taken as a
/// Poisson process along the path.
pub fn encounter_probability(mines: &[(f64, f64)]) -> f64 {
    let expected: f64 = mines.iter().map(|(density, width)| density * width).sum();
    1.0 - (-expected).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FM 20-32 (1998) ch. 6, figures 6-10 to 6-12, standardised row minefields (front m,
    /// full-width mines, track-width mines) against the encounter bands of ch. 2–3 (disrupt
    /// about 50 %, fix 50–60 %, turn about 80 %, block 85 % and more). Effective widths 2.0 m
    /// (full width) and 1.0 m (track width), as in `obstacles.ron`.
    #[test]
    fn standard_minefields_give_the_manuals_encounter_rates() {
        let p = |front: f64, fw: f64, tw: f64| encounter_probability(&[(fw / front, 2.0), (tw / front, 1.0)]);
        let disrupt = p(250.0, 42.0, 84.0);
        let fix = p(250.0, 63.0, 84.0);
        let turn = p(500.0, 336.0, 168.0);
        let block = p(500.0, 378.0, 168.0);
        assert!((0.45..=0.55).contains(&disrupt), "disrupt {disrupt:.2}");
        assert!((0.50..=0.60).contains(&fix), "fix {fix:.2}");
        assert!((0.75..=0.85).contains(&turn), "turn {turn:.2}");
        assert!((0.83..=0.90).contains(&block), "block {block:.2}");
        assert!(disrupt < fix && fix < turn && turn < block);
    }
}
