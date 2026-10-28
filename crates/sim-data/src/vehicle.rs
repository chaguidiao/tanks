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
    /// Role class: ammunition selection and target priority use it.
    pub class: VehicleClass,
    pub dimensions: Dimensions,
    pub mobility: MobilityDef,
    pub gun: GunLimits,
    pub crew: u8,
    pub sensors: Vec<SensorDef>,
    pub fire_control: FireControlDef,
    pub armament: ArmamentDef,
    pub armour: ArmourDef,
    pub ammo_stowage: AmmoStowage,
    /// Self-screening smoke grenade launchers.
    #[serde(default)]
    pub smoke_launcher: Option<crate::SmokeLauncherDef>,
    /// Engine exhaust smoke.
    #[serde(default)]
    pub engine_smoke: Option<crate::EngineSmokeDef>,
    /// Coaxial machine gun (bursts at infantry, TODO B3).
    #[serde(default)]
    pub coax: Option<crate::SmallArmDef>,
    /// Troop compartment (IFVs).
    #[serde(default)]
    pub troops: Option<crate::TroopsDef>,
}

/// What a vehicle is, as a crew would call it when recognising it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VehicleClass {
    /// Main battle tank.
    Mbt,
    /// Infantry fighting vehicle (autocannon, often an ATGM).
    Ifv,
    /// Light reconnaissance vehicle.
    Recon,
}

/// What a round is meant for: a vehicle class or dismounted infantry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetClass {
    Mbt,
    Ifv,
    Recon,
    Infantry,
}

impl From<VehicleClass> for TargetClass {
    fn from(c: VehicleClass) -> Self {
        match c {
            VehicleClass::Mbt => TargetClass::Mbt,
            VehicleClass::Ifv => TargetClass::Ifv,
            VehicleClass::Recon => TargetClass::Recon,
        }
    }
}

impl VehicleClass {
    pub fn label(self) -> &'static str {
        match self {
            VehicleClass::Mbt => "TANK",
            VehicleClass::Ifv => "IFV",
            VehicleClass::Recon => "RECON",
        }
    }
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
    /// Stand-off round (plan M8B-AT-TEAMS §5, B6): beyond this range the crew fires it first at a
    /// target it defeats; inside it, only when no other round does (m).
    #[serde(default)]
    pub prefer_beyond_m: Option<Stat<f64>>,
    /// Years this stowage entry applies (N11): a vehicle whose round changes mid-service lists one
    /// entry per period, so the overlap years do not carry both. None = whenever the round is in
    /// service.
    #[serde(default)]
    pub in_service: Option<ServiceDates>,
}

impl AmmoLoad {
    /// This entry is stowed in `year` (its own dates only; the round's dates are checked apart).
    pub fn stowed_in(&self, year: i32) -> bool {
        self.in_service.is_none_or(|d| d.contains(year))
    }
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
    /// Multiplier from the KE values below to RHA equivalence against shaped charges
    /// (composite and special armour defeat jets better than rods).
    pub ce_factor: Stat<f64>,
    /// Explosive reactive armour fitted to the front and sides.
    #[serde(default)]
    pub era: Option<EraDef>,
    pub turret_front: Stat<f64>,
    pub turret_side: Stat<f64>,
    pub turret_rear: Stat<f64>,
    pub hull_front: Stat<f64>,
    pub hull_side: Stat<f64>,
    pub hull_rear: Stat<f64>,
    /// Turret roof and engine deck, met by plunging fire from above and by top-attack missiles
    /// (plan M8B-AT-TEAMS §3). Plain steel: `ce_factor` does not apply.
    pub top: Stat<f64>,
}

/// Explosive reactive armour: a finite number of tiles on the front and sides. A hit on a
/// covered spot fires the tile, which disrupts a jet (and slightly a rod) and is then spent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EraDef {
    pub in_service: ServiceDates,
    /// Tiles carried; each spent by one hit.
    pub tiles: u32,
    /// Probability that a hit on the front or a side strikes a tile (rear is not covered).
    pub coverage: Stat<f64>,
    /// Fraction of shaped-charge penetration removed when a tile fires.
    pub ce_defeat: Stat<f64>,
    /// RHA equivalence added against kinetic rounds when a tile fires (mm).
    pub ke_gain_mm: Stat<f64>,
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
    /// Long-rod sabot (Lanz–Odermatt penetration).
    Apfsds,
    /// Armour-piercing discarding sabot from an autocannon: flat penetration scaled by v².
    Apds,
    /// Shaped charge (HEAT, also guided missiles): flat penetration, fights CE armour and ERA.
    Heat,
    /// High-explosive fragmentation: defeats only light armour, otherwise a non-penetrating hit.
    HeFrag,
    /// Smoke shell: no effect on target, makes clouds.
    Smoke,
}

impl AmmoKind {
    pub const ALL: [AmmoKind; 5] =
        [AmmoKind::Apfsds, AmmoKind::Apds, AmmoKind::Heat, AmmoKind::HeFrag, AmmoKind::Smoke];

    /// Shaped-charge rounds meet the chemical-energy armour values.
    pub fn is_chemical(self) -> bool {
        matches!(self, AmmoKind::Heat | AmmoKind::HeFrag)
    }

    pub fn label(self) -> &'static str {
        match self {
            AmmoKind::Apfsds => "APFSDS",
            AmmoKind::Apds => "APDS",
            AmmoKind::Heat => "HEAT",
            AmmoKind::HeFrag => "HE-FRAG",
            AmmoKind::Smoke => "SMOKE",
        }
    }
}

/// What a smoke shell makes where it lands.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmmoSmoke {
    /// Id in `data/combat/smoke.ron`.
    pub kind: String,
    /// Clouds per round (a shell makes one; a base-ejection round a few).
    pub clouds: u32,
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
    /// Long-rod geometry (APFSDS only).
    #[serde(default)]
    pub rod_length_mm: Option<Stat<f64>>,
    #[serde(default)]
    pub rod_diameter_mm: Option<Stat<f64>>,
    #[serde(default)]
    pub rod_density_g_cm3: Option<Stat<f64>>,
    /// RHA penetration at the muzzle for APDS / HEAT / HE-FRAG (mm).
    #[serde(default)]
    pub flat_penetration_mm: Option<Stat<f64>>,
    /// Time to ready the next round after firing this one, where it differs from the
    /// weapon's reload (guided missiles, heavy rounds) (s).
    #[serde(default)]
    pub reload_s: Option<Stat<f64>>,
    /// Longest range this round is fired at (m); None = the weapon's limit.
    #[serde(default)]
    pub max_range_m: Option<Stat<f64>>,
    /// Target classes the crew picks this round for, in the AI's first choice.
    #[serde(default)]
    pub preferred_vs: Vec<TargetClass>,
    /// Radius within which the burst incapacitates men standing in the open (HE and
    /// multipurpose rounds; plan M8B-INFANTRY §4).
    #[serde(default)]
    pub lethal_radius_m: Option<Stat<f64>>,
    /// Command-guided round (ATGM): flies to the aim point the gunner keeps on the target, so
    /// there is no superelevation (ranging) error, no ballistic bias and no lead error; only the
    /// gunner's tracking error and the missile's own dispersion remain.
    #[serde(default)]
    pub guided: bool,
    /// How the round is stowed when resupplied; None = a missile if guided, else a main-gun
    /// round (autocannon rounds and gun-launched missiles say so in their data).
    #[serde(default)]
    pub stow: Option<crate::StowClass>,
    #[serde(default)]
    pub smoke: Option<AmmoSmoke>,
    /// Tandem shaped charge: the precursor fires a reactive tile, which is spent but does not
    /// disrupt the main jet (plan M8B-AT-TEAMS §2.4).
    #[serde(default)]
    pub tandem: bool,
    /// Share of a penetration's catastrophic, firepower and crew outcomes this round achieves
    /// (small warheads make a narrow path behind the armour; plan M8B-AT-TEAMS §2.3). Mobility
    /// outcomes keep their full value. None = 1.
    #[serde(default)]
    pub behind_armour: Option<Stat<f64>>,
    /// Guided round whose seeker flies it to the target after launch: the gunner does not need
    /// to keep tracking (plan M8B-AT-TEAMS §2.2).
    #[serde(default)]
    pub fire_and_forget: bool,
    /// Top-attack missile: dives onto the roof at this angle below the horizontal (deg).
    #[serde(default)]
    pub top_attack_deg: Option<Stat<f64>>,
    /// Shortest range at which the round arms or is gathered into guidance (m).
    #[serde(default)]
    pub min_range_m: Option<Stat<f64>>,
    /// Round-to-round dispersion, 1σ (mrad).
    pub dispersion_mrad: Stat<f64>,
    /// Published/estimated penetration at 2 km into RHA at normal (mm), for validation only.
    #[serde(default)]
    pub reference_penetration_2km_mm: Option<Stat<f64>>,
}

impl AmmoDef {
    /// Behind-armour factor (see `behind_armour`).
    pub fn behind_armour_factor(&self) -> f64 {
        self.behind_armour.as_ref().map_or(1.0, |b| b.get())
    }

    /// Stow class for resupply (see `stow`).
    pub fn stow_class(&self) -> crate::StowClass {
        self.stow.unwrap_or(if self.guided { crate::StowClass::Missile } else { crate::StowClass::MainGun })
    }

    /// Rod length, diameter and density for long-rod rounds.
    pub fn rod(&self) -> Option<(f64, f64, f64)> {
        Some((
            self.rod_length_mm.as_ref()?.get(),
            self.rod_diameter_mm.as_ref()?.get(),
            self.rod_density_g_cm3.as_ref()?.get(),
        ))
    }
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
