//! Dismounted infantry (docs/plans/M8B-INFANTRY.md §0–§4): squad types, small arms and the
//! infantry combat model. A squad is one abstract element with a strength, three postures and one
//! burst-fire model (DESIGN.md §14 #29).

use serde::{Deserialize, Serialize};

use crate::{SensorDef, ServiceDates, Stat};

/// A squad type (`data/infantry/*.ron`). One squad is what one IFV carries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SquadDef {
    pub id: String,
    /// Display name (an organisation, not a person).
    pub name: String,
    /// The real organisation it mirrors (documentation only).
    pub mirrors: String,
    pub in_service: ServiceDates,
    /// Men at full strength.
    pub men: u8,
    /// Weapons carried; each entry may carry its own in-service dates (era is data).
    pub weapons: Vec<SmallArmDef>,
    /// Eyes, binoculars and night sights.
    pub sensors: Vec<SensorDef>,
    pub signature: SquadSignature,
    /// Time halted in the open before the squad counts as dug in (hasty fighting positions).
    pub dig_in_s: Stat<f64>,
    /// Body armour by period (calibration notes §3); none listed = no armour.
    #[serde(default)]
    pub body_armour: Vec<BodyArmourDef>,
    /// Night aiming aids by period (aiming lasers with goggles); none = unaided at night.
    #[serde(default)]
    pub aiming_aids: Vec<AimingAidDef>,
}

impl SquadDef {
    /// Weapons in service in `year`.
    pub fn weapons_in(&self, year: i32) -> impl Iterator<Item = &SmallArmDef> {
        self.weapons.iter().filter(move |w| w.in_service.is_none_or(|d| d.contains(year)))
    }

    /// Body armour worn in `year` (the last listed row in service).
    pub fn armour_in(&self, year: i32) -> Option<&BodyArmourDef> {
        self.body_armour.iter().rev().find(|a| a.in_service.contains(year))
    }

    /// Night aiming aid carried in `year` (the last listed row in service).
    pub fn aiming_aid_in(&self, year: i32) -> Option<&AimingAidDef> {
        self.aiming_aids.iter().rev().find(|a| a.in_service.contains(year))
    }
}

/// Body armour of a squad in one period: multipliers on the chance that a bullet hit takes a man
/// out of the fight and on HE fragment exposure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BodyArmourDef {
    pub name: String,
    pub in_service: ServiceDates,
    pub bullet_factor: Stat<f64>,
    pub fragment_factor: Stat<f64>,
}

/// A night aiming aid (an infrared aiming laser seen through goggles): at night the squad's
/// small-arms aiming error grows by `night_factor` instead of the model's unaided factor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AimingAidDef {
    pub name: String,
    pub in_service: ServiceDates,
    pub night_factor: Stat<f64>,
}

/// What the enemy sees and shoots at in one posture.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostureSignature {
    /// Size of the visible group: detection uses `sqrt(width × height)` as the critical
    /// dimension, as for vehicles.
    pub width_m: Stat<f64>,
    pub height_m: Stat<f64>,
    /// Area one man presents to aimed fire (hit probability of a burst).
    pub man_area_m2: Stat<f64>,
    /// Observation height (m above the ground, or above the floor in a building).
    pub eye_height_m: Stat<f64>,
}

/// Signature rows per posture.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SquadSignature {
    pub moving: PostureSignature,
    /// Halted: kneeling and prone.
    pub halted: PostureSignature,
    pub dug_in: PostureSignature,
    /// Occupying a building: seen and fired at through windows on the facade facing the enemy.
    pub in_building: PostureSignature,
}

/// A small arm or machine gun, resolved as bursts (plan §4: one burst = one draw).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmallArmDef {
    pub name: String,
    #[serde(default)]
    pub in_service: Option<ServiceDates>,
    /// Weapons of this type in the squad (1 for a vehicle's coax).
    pub count: u8,
    pub rounds_per_burst: Stat<f64>,
    /// Time between bursts of one weapon in sustained fire (s).
    pub burst_interval_s: Stat<f64>,
    /// Longest range it is fired at (m).
    pub max_range_m: Stat<f64>,
    /// 1σ aiming and burst dispersion at the target (mrad).
    pub dispersion_mrad: Stat<f64>,
    /// Basic load per weapon, in bursts.
    pub bursts: u32,
}

/// Troop compartment of an IFV (plan §2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TroopsDef {
    /// Seats for dismounts (men).
    pub capacity_men: u8,
    /// Ramp or door drill: from halt to the squad out and in its security posture (s).
    pub dismount_s: Stat<f64>,
    /// From the squad at the vehicle to the ramp closed (s).
    pub mount_s: Stat<f64>,
}

/// Exposure to HE fragments per posture (1 = standing in the open).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostureFactors {
    pub moving: Stat<f64>,
    pub halted: Stat<f64>,
    pub dug_in: Stat<f64>,
}

/// Mounted squads lost with their carrier (plan §2): fraction of the men lost for each result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountedLoss {
    pub catastrophic: Stat<f64>,
    /// Penetration that does not destroy the vehicle.
    pub penetration: Stat<f64>,
}

/// Tall crops in season hide prone infantry (TODO B8, TERRAIN.md §5.3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TallCrops {
    /// Months (1–12) when crop land stands tall.
    pub months: Vec<u8>,
    /// Visible fraction of a halted or dug-in squad in standing crops.
    pub visible_fraction: Stat<f64>,
}

/// Infantry combat model constants (`data/combat/infantry.ron`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfantryModel {
    /// Probability that a hit incapacitates a man.
    pub lethality_per_hit: Stat<f64>,
    /// A single round never hits with more than this probability.
    pub max_round_hit: Stat<f64>,
    pub he_exposure: PostureFactors,
    /// Radius over which a squad's men are spread (m).
    pub spread_radius_m: Stat<f64>,
    /// Suppression added to a squad per 10 rounds aimed at it (0..1).
    pub suppression_per_burst: Stat<f64>,
    /// Suppression added to a vehicle by small-arms fire (unbuttoned commander forced down).
    pub vehicle_suppression_per_burst: Stat<f64>,
    /// Above this a squad does not move (it still fires, slower and less accurately).
    pub pinned_above: Stat<f64>,
    /// Squads fire their small arms at vehicles (to drive exposed crews down) inside this range,
    /// when no infantry is in reach (m).
    pub vehicle_engage_m: Stat<f64>,
    /// How much of the vehicles' thermal crossover applies to people (0 = none, 1 = all).
    pub thermal_crossover_share: Stat<f64>,
    /// Multiplier on optical (visible-band) cycles against men: camouflage clothing and
    /// vegetation give far less contrast than a vehicle's hull and shadow.
    pub optical_contrast: Stat<f64>,
    pub tall_crops: TallCrops,
    pub mounted_loss: MountedLoss,
    /// Small-arms aiming error multiplier in full darkness without an aiming aid; scaled by
    /// darkness (1 − light), so it builds through dusk.
    pub night_aiming_factor: Stat<f64>,
}

impl InfantryModel {
    /// Single-round hit probability on one man of area `man_area_m2` at range `r` with 1σ
    /// dispersion `sigma_mrad` (circular normal: the target area over the error ellipse
    /// area 2πσ², capped).
    pub fn round_hit(&self, man_area_m2: f64, sigma_mrad: f64, r: f64) -> f64 {
        let s = sigma_mrad * r / 1000.0;
        (man_area_m2 / (std::f64::consts::TAU * s * s)).min(self.max_round_hit.get())
    }
}

/// Infantry block of a doctrine pack (plan §6).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfantryDoctrine {
    /// Dismounted squads take up security this far from their carrier (m).
    pub security_radius_m: Stat<f64>,
    /// React to contact: a squad under fire in the open looks for cover this far away (m).
    pub cover_search_m: Stat<f64>,
    /// Below this fraction of its men a squad is combat-ineffective.
    pub ineffective_fraction: Stat<f64>,
    /// Crews engage infantry inside this range before more distant vehicles (m).
    pub close_infantry_m: Stat<f64>,
    /// Squad spacing in a dismounted platoon line (m).
    pub squad_spacing_m: Stat<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ds() -> crate::DataSet {
        crate::DataSet::load(&std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")).unwrap()
    }

    fn model() -> InfantryModel {
        ds().infantry
    }

    #[test]
    fn squads_load_and_fit_their_carriers() {
        let ds = ds();
        for (squad, carrier) in [("rifle_squad_b", "ward_m2"), ("mr_squad", "skif_m1"), ("mr_squad", "skif_m0")] {
            let s = ds.squad(squad).expect(squad);
            let t = ds.vehicle(carrier).and_then(|v| v.troops.as_ref()).expect(carrier);
            assert!(s.men <= t.capacity_men, "{squad} ({} men) fits {carrier} ({})", s.men, t.capacity_men);
        }
        // Tanks carry no troops; every vehicle has a coax.
        assert!(ds.vehicle("bulwark_a1").unwrap().troops.is_none());
        assert!(ds.vehicles.iter().all(|v| v.coax.is_some()), "every vehicle has a coax MG");
    }

    #[test]
    fn squad_weapons_follow_the_year() {
        // Era is data: the Blue squad's machine gun is the M60 class until 1984, the SAW after.
        let s = ds().squad("rifle_squad_b").unwrap().clone();
        let names = |y: i32| s.weapons_in(y).map(|w| w.name.clone()).collect::<Vec<_>>();
        assert!(names(1982).iter().any(|n| n.contains("7.62")) && !names(1982).iter().any(|n| n.contains("SAW")));
        assert!(names(1990).iter().any(|n| n.contains("SAW")) && !names(1990).iter().any(|n| n.contains("7.62")));
        let men: u8 = s.weapons_in(1990).map(|w| w.count).sum();
        assert_eq!(men, s.men, "one weapon per man");
    }

    #[test]
    fn round_hit_falls_with_the_square_of_range() {
        let m = model();
        // 0.75 m² man, 3 mrad: at 300 m σ = 0.9 m, P = 0.75 / (2π · 0.81) = 0.147.
        let p = m.round_hit(0.75, 3.0, 300.0);
        assert!((p - 0.75 / (std::f64::consts::TAU * 0.81)).abs() < 1e-9, "{p}");
        assert!((m.round_hit(0.75, 3.0, 600.0) - p / 4.0).abs() < 1e-9);
        assert!(m.round_hit(0.75, 3.0, 5.0) <= m.max_round_hit.get());
    }

    #[test]
    fn round_hit_matches_hitchman_range_trials() {
        // Hitchman, ORO-T-160 (1952): marksmen firing alone at 300 yd (274 m) hit a type E
        // silhouette (0.43 m²) 24 % of the time; their aiming error is about 2 mrad per axis.
        // Calibration notes §1.3 V1: 0.24 ± 0.03.
        let p = model().round_hit(0.43, 2.0, 274.3);
        assert!((p - 0.24).abs() <= 0.03, "{p}");
    }

    #[test]
    fn era_rows_resolve_by_year() {
        // Calibration notes §3: red-dot rifles from 2003, plates from 2004, aiming laser from 2002.
        let b = ds().squad("rifle_squad_b").unwrap().clone();
        let rifle = |y: i32| b.weapons_in(y).find(|w| w.name.contains("RIFLES")).unwrap().dispersion_mrad.get();
        assert_eq!((rifle(2002), rifle(2003)), (6.0, 5.0));
        assert!(b.armour_in(1980).is_none());
        assert_eq!(b.armour_in(1990).unwrap().bullet_factor.get(), 1.0);
        assert!(b.armour_in(2005).unwrap().bullet_factor.get() < 0.9);
        assert!(b.aiming_aid_in(1995).is_none() && b.aiming_aid_in(2005).is_some());
        let r = ds().squad("mr_squad").unwrap().clone();
        assert!(r.aiming_aid_in(2005).is_none());
        assert!(r.armour_in(2005).unwrap().fragment_factor.get() > b.armour_in(2005).unwrap().fragment_factor.get());
    }
}
