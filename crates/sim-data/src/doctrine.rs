//! Doctrine packs (`data/doctrine/<pack>/doctrine.ron`). A scenario assigns one per side;
//! national doctrines are additional packs, never code branches (CLAUDE.md doctrine rules).

use serde::{Deserialize, Serialize};

use crate::Stat;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Formation {
    Column,
    StaggeredColumn,
    Wedge,
    Vee,
    Line,
    EchelonLeft,
    EchelonRight,
}

impl Formation {
    pub const ALL: [Formation; 7] = [
        Formation::Column,
        Formation::StaggeredColumn,
        Formation::Wedge,
        Formation::Vee,
        Formation::Line,
        Formation::EchelonLeft,
        Formation::EchelonRight,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Technique {
    Traveling,
    TravelingOverwatch,
    BoundingOverwatch,
}

/// Slot offsets in units of `spacing_m`: (right, forward) relative to the platoon leader,
/// listed in platoon order (vehicle 1 = leader).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormationDef {
    pub formation: Formation,
    pub slots: Vec<(f64, f64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctrinePack {
    pub id: String,
    pub name: String,
    pub source: String,
    pub formations: Vec<FormationDef>,
    pub default_formation: Formation,
    /// Interval between vehicles (m).
    pub spacing_m: Stat<f64>,
    /// Bound length for bounding overwatch (m).
    pub bound_length_m: Stat<f64>,
    /// Lead-section distance ahead of the trail section in traveling overwatch (m).
    pub overwatch_distance_m: Stat<f64>,
    /// Delay from a crew sighting to the spot report reaching the platoon/company picture (s).
    pub report_delay_min_s: Stat<f64>,
    pub report_delay_max_s: Stat<f64>,
    /// Delay for an order to be passed, acknowledged and acted on (s).
    pub order_delay_min_s: Stat<f64>,
    pub order_delay_max_s: Stat<f64>,
    /// Open fire at targets inside this range (m), FREE weapons.
    pub engagement_range_m: Stat<f64>,
    /// React to contact: halt and fight from the current position when moving.
    pub halt_on_contact: bool,
    /// Lateral spacing between vehicles in a battle position (m).
    pub bp_dispersion_m: Stat<f64>,
    /// Commander-level templates (OPFOR attack): distances to the objective at which the
    /// march column deploys to pre-battle and then battle formation (m).
    pub prebattle_distance_m: Stat<f64>,
    pub battle_distance_m: Stat<f64>,
    pub march_formation: Formation,
    pub prebattle_formation: Formation,
    pub battle_formation: Formation,
    /// Fraction of a platoon lost after which it stops advancing and fights in place.
    pub break_off_losses: Stat<f64>,
    /// March speeds (m/s) the commander can order on a move, fastest first; "MAX" (no
    /// limit) is always offered as well.
    #[serde(default)]
    pub speed_limits_mps: Vec<Stat<f64>>,
    /// Crews employ self-screening smoke when fired upon (ATP 3-20.15 App. F, break contact).
    pub smoke_when_engaged: bool,
    /// Distance a platoon withdraws from the threat in the break-contact drill (m).
    pub break_contact_distance_m: Stat<f64>,
    /// Pre-arranged code words (FM 6-0 execution matrix): the index is the word's number on the
    /// command card (Ctrl+1..9).
    #[serde(default)]
    pub code_words: Vec<String>,
}

impl DoctrinePack {
    pub fn formation(&self, f: Formation) -> &FormationDef {
        self.formations.iter().find(|d| d.formation == f).unwrap_or(&self.formations[0])
    }
}
