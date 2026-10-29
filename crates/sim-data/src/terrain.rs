use serde::{Deserialize, Serialize};

use crate::Stat;

/// Land-cover class table (`data/terrain/landcover.ron`). Ids are stable u8 raster values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LandCoverDef {
    pub id: u8,
    pub name: String,
    /// Display fill colour (RGB). Desaturated; the renderer may further modulate it.
    pub display_rgb: [u8; 3],
    /// Multiplier on achievable cross-country speed from vegetation/obstacles (0 = no-go).
    pub speed_factor: Stat<f64>,
    /// Micro-relief amplitude used by `h_micro` (m).
    pub micro_relief_m: Stat<f64>,
    /// Height of vegetation/obstructions above ground that sight lines must pass (m).
    pub canopy_height_m: Stat<f64>,
    /// Beer–Lambert attenuation inside the canopy volume (per metre of path).
    pub attenuation_per_m: Stat<f64>,
    /// Multiplier on foot cross-country speed (0 = no-go on foot). Vegetation that stops a tank
    /// only slows a squad.
    pub foot_speed_factor: Stat<f64>,
    #[serde(default)]
    pub is_forest: bool,
    #[serde(default)]
    pub is_water: bool,
    /// Settled ground (villages, towns): tank-only platoons route round it (plan
    /// M8F1-INFANTRY-AI §3).
    #[serde(default)]
    pub built_up: bool,
}

/// Soil class table (`data/terrain/soils.ron`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoilDef {
    pub id: u8,
    pub name: String,
    /// Tracked-vehicle rolling resistance coefficient (dimensionless).
    pub rolling_resistance: Stat<f64>,
    /// Traction coefficient: max tractive force / weight.
    pub traction_coefficient: Stat<f64>,
}

/// Building construction class, as authored on each footprint (TERRAIN.md §5.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildingClass {
    Wood,
    Masonry,
    Concrete,
    Industrial,
}

impl BuildingClass {
    pub const ALL: [BuildingClass; 4] = [Self::Wood, Self::Masonry, Self::Concrete, Self::Industrial];
}

/// What a building protects its occupants against (plan M8B-INFANTRY §5, H7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildingThreat {
    /// Rifle and machine-gun fire up to 7.62 mm.
    SmallArms,
    /// Fragments from HE bursting outside the building.
    Fragments,
    /// Blast of HE bursting on or in the building.
    Blast,
}

/// Fraction of the casualties an occupant would take in the open that still occur inside
/// (1 = no protection, 0 = full protection).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildingProtection {
    pub small_arms: Stat<f64>,
    pub fragments: Stat<f64>,
    pub blast: Stat<f64>,
}

/// Building construction table entry (`data/terrain/buildings.ron`, TODO H7).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildingTypeDef {
    pub class: BuildingClass,
    pub wall_thickness_m: Stat<f64>,
    /// Storey height including a share of the roof, used to count floors from the footprint height.
    pub floor_height_m: Stat<f64>,
    pub protection: BuildingProtection,
    /// Squads that can occupy and fight from 100 m² of floor area.
    pub squads_per_100m2: Stat<f64>,
    /// Most squads one building holds, however large (plan M8H1B-TOWNS §7 #5).
    pub max_squads: Stat<f64>,
}

/// Foot movement for dismounted infantry planning (`data/terrain/foot.ron`, TODO H9).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FootMobilityDef {
    pub road_speed_mps: Stat<f64>,
    pub cross_country_speed_mps: Stat<f64>,
    /// Steepest slope (rad) a loaded squad still climbs.
    pub max_slope: Stat<f64>,
    pub fording_depth_m: Stat<f64>,
    /// Movement through rooms, doors and stairs.
    pub building_interior_speed_mps: Stat<f64>,
}

#[derive(Debug, Clone)]
pub struct TerrainData {
    pub landcover: Vec<LandCoverDef>,
    pub soils: Vec<SoilDef>,
    pub buildings: Vec<BuildingTypeDef>,
    pub foot: FootMobilityDef,
}

impl TerrainData {
    pub fn validate(&self) -> Result<(), String> {
        for (i, lc) in self.landcover.iter().enumerate() {
            if lc.id as usize != i {
                return Err(format!("landcover ids must be 0..n in order; '{}' has id {}", lc.name, lc.id));
            }
        }
        for (i, s) in self.soils.iter().enumerate() {
            if s.id as usize != i {
                return Err(format!("soil ids must be 0..n in order; '{}' has id {}", s.name, s.id));
            }
        }
        for c in BuildingClass::ALL {
            let n = self.buildings.iter().filter(|b| b.class == c).count();
            if n != 1 {
                return Err(format!("buildings.ron must define building class {c:?} exactly once (found {n})"));
            }
        }
        Ok(())
    }

    pub fn building(&self, class: BuildingClass) -> &BuildingTypeDef {
        self.buildings.iter().find(|b| b.class == class).expect("validated: every class defined")
    }

    /// Floors in a building of this class and height (at least one).
    pub fn floors(&self, class: BuildingClass, height_m: f64) -> u32 {
        let fh = self.building(class).floor_height_m.get();
        ((height_m / fh).floor() as u32).max(1)
    }

    /// Squads that fit in a building (at least one, at most the class's cap): floor area over
    /// all floors times the class's density.
    pub fn squad_capacity(&self, class: BuildingClass, footprint_m2: f64, height_m: f64) -> u32 {
        let def = self.building(class);
        let area = footprint_m2 * self.floors(class, height_m) as f64;
        let n = (area / 100.0 * def.squads_per_100m2.get()).floor() as u32;
        n.min(def.max_squads.get() as u32).max(1)
    }

    /// Casualty multiplier for occupants of a building of this class against a threat.
    pub fn protection(&self, class: BuildingClass, threat: BuildingThreat) -> f64 {
        let p = &self.building(class).protection;
        match threat {
            BuildingThreat::SmallArms => p.small_arms.get(),
            BuildingThreat::Fragments => p.fragments.get(),
            BuildingThreat::Blast => p.blast.get(),
        }
    }

    pub fn landcover(&self, id: u8) -> &LandCoverDef {
        &self.landcover[(id as usize).min(self.landcover.len() - 1)]
    }

    pub fn soil(&self, id: u8) -> &SoilDef {
        &self.soils[(id as usize).min(self.soils.len() - 1)]
    }

    pub fn landcover_id(&self, name: &str) -> Option<u8> {
        self.landcover.iter().find(|l| l.name == name).map(|l| l.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn td() -> TerrainData {
        crate::DataSet::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")).unwrap().terrain
    }

    #[test]
    fn every_building_class_has_a_type() {
        let td = td();
        for c in BuildingClass::ALL {
            assert_eq!(td.building(c).class, c);
        }
    }

    #[test]
    fn floors_and_capacity_follow_height_and_area() {
        let td = td();
        // Masonry storeys are 3.5 m: a generated 4.5 m house has one floor, a 9 m one two.
        assert_eq!(td.floors(BuildingClass::Masonry, 4.5), 1);
        assert_eq!(td.floors(BuildingClass::Masonry, 9.0), 2);
        assert_eq!(td.floors(BuildingClass::Masonry, 2.0), 1, "at least one floor");
        // 12 x 9 m two-storey house: 216 m² of floor, one squad per 100 m² → 2 squads.
        assert_eq!(td.squad_capacity(BuildingClass::Masonry, 108.0, 9.0), 2);
        // A small shed still holds one squad.
        assert_eq!(td.squad_capacity(BuildingClass::Wood, 20.0, 3.0), 1);
        // Industrial halls: one tall floor at half the density.
        assert_eq!(td.squad_capacity(BuildingClass::Industrial, 800.0, 8.0), 4);
        // A 60 x 12 m five-storey slab block: 3600 m² of floor, capped at four squads.
        assert_eq!(td.squad_capacity(BuildingClass::Concrete, 720.0, 16.0), 4);
    }

    #[test]
    fn protection_ranks_the_classes_as_fm_90_10_1_does() {
        // FM 90-10-1 (1993) ch. 7: wood-frame walls do not stop small-arms fire, brick and
        // reinforced concrete do; concrete resists blast best.
        let td = td();
        use BuildingThreat::*;
        let p = |c, t| td.protection(c, t);
        assert!(p(BuildingClass::Masonry, SmallArms) < p(BuildingClass::Wood, SmallArms));
        assert!(p(BuildingClass::Concrete, Blast) < p(BuildingClass::Masonry, Blast));
        for c in BuildingClass::ALL {
            for t in [SmallArms, Fragments, Blast] {
                let v = p(c, t);
                assert!(v > 0.0 && v <= 1.0, "{c:?} {t:?} = {v}: occupants are never fully safe");
            }
        }
    }

    #[test]
    fn forest_slows_a_squad_less_than_a_tank() {
        let td = td();
        for name in ["deciduous_forest", "coniferous_forest", "marsh"] {
            let lc = td.landcover(td.landcover_id(name).unwrap());
            assert!(lc.foot_speed_factor.get() > lc.speed_factor.get(), "{name}");
        }
    }
}
