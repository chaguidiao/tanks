//! Gunnery: target selection, fire-control timeline, hit probability, terminal effects
//! (DESIGN.md §7.2–7.4).
//!
//! Timeline (TC 3-20.31 engagement sequence): acquire → traverse/lay → lase → fire →
//! observe → reload → re-engage. Hits come from an error budget sampled per round against the
//! target's *visible* silhouette, so hull-down targets are harder to hit. Penetration is the
//! Lanz–Odermatt long-rod form; armour is per face with line-of-sight obliquity.

use ai::picture::{Condition, ContactLevel, WeaponsStatus};
use rand_chacha::ChaCha8Rng;
use sim_data::{AmmoDef, AmmoKind, DamageTables, FireControlDef, Outcomes, PenetrationModel, SmokeModel, VehicleDef};
use terrain::hash::StableHasher;

use crate::command::KillKind;
use crate::picture::gauss;
use crate::sensors::{projected_width, uniform};
use crate::world::World;
use crate::{DT, dist};

pub const G: f64 = 9.80665;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Phase {
    Ready,
    Laying { left: f64 },
    Lasing { left: f64 },
    Reloading { left: f64 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct GunState {
    /// Turret azimuth (compass rad).
    pub turret: f64,
    /// Rounds left per entry of the vehicle's loadout (`armament.load` order).
    pub stock: Vec<u32>,
    /// Rounds carried when full, per loadout entry (the stock at the start; resupply fills to it).
    pub capacity: Vec<u32>,
    /// Round type the commander ordered; None = the crew picks by target class.
    pub selected: Option<AmmoKind>,
    /// Smoke shells to fire at a point (a fire mission); takes precedence over targets.
    pub mission: Option<SmokeMission>,
    /// World index of the current target (from the crew's own tracks).
    pub target: Option<usize>,
    /// Target given by the platoon fire plan or a micro-order.
    pub assigned: Option<usize>,
    pub phase: Phase,
    /// Rounds fired at the current target (first round vs. adjusted fire).
    pub rounds_at_target: u32,
    /// Per-engagement bias (vertical, horizontal) in mrad.
    pub bias: (f64, f64),
    /// Crew casualties slow every action.
    pub crew_loss: u8,
    /// Infantry target the gunner engages with the coax this tick (set by `step`, read by
    /// `smallarms::step`).
    pub coax_target: Option<usize>,
    /// The crew is stowing main-gun rounds from a resupply until this tick: the main gun does not
    /// fire (plan M8C-TAILGATE-TRUCKS §8 #6). 0 = never.
    pub loading_until: u64,
}

/// A smoke fire mission (point target), FM 3-11.50.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmokeMission {
    pub at: [f64; 2],
    pub rounds_left: u32,
}

impl GunState {
    pub fn new(heading: f64, stock: Vec<u32>) -> Self {
        Self {
            turret: heading,
            capacity: stock.clone(),
            stock,
            selected: None,
            mission: None,
            target: None,
            assigned: None,
            phase: Phase::Ready,
            rounds_at_target: 0,
            bias: (0.0, 0.0),
            crew_loss: 0,
            coax_target: None,
            loading_until: 0,
        }
    }

    /// Rounds left, all types.
    pub fn rounds(&self) -> u32 {
        self.stock.iter().sum()
    }

    pub(crate) fn hash_into(&self, h: &mut StableHasher) {
        h.f64(self.turret);
        for &n in &self.stock {
            h.u32(n);
        }
        h.u8(self.selected.map_or(255, |k| k as u8));
        match self.mission {
            Some(m) => {
                h.f64(m.at[0]);
                h.f64(m.at[1]);
                h.u32(m.rounds_left);
            }
            None => h.u8(0),
        }
        h.u64(self.target.map_or(u64::MAX, |t| t as u64));
        h.u32(self.rounds_at_target);
        h.f64(self.bias.0);
        h.f64(self.bias.1);
        // Only once a vehicle has been resupplied, so older states hash as before.
        if self.loading_until > 0 {
            h.u64(self.loading_until);
        }
    }
}

// --- ballistics ---------------------------------------------------------------------------

/// Velocity at range with linear velocity decay (m/s).
pub fn velocity_at(a: &AmmoDef, r: f64) -> f64 {
    (a.muzzle_velocity_mps.get() - a.velocity_loss_mps_per_km.get() * r / 1000.0).max(100.0)
}

/// Time of flight for linear velocity decay: t = ln(v0 / v(r)) / k, k = loss per metre.
pub fn time_of_flight(a: &AmmoDef, r: f64) -> f64 {
    let v0 = a.muzzle_velocity_mps.get();
    let k = a.velocity_loss_mps_per_km.get() / 1000.0;
    if k <= 0.0 { r / v0 } else { libm::log(v0 / velocity_at(a, r)) / k }
}

/// Gravity drop the fire-control system must correct (m).
pub fn drop_at(a: &AmmoDef, r: f64) -> f64 {
    if a.guided {
        return 0.0;
    }
    let t = time_of_flight(a, r);
    0.5 * G * t * t
}

/// Superelevation sensitivity to range error: d(drop/r)/dr in mrad per metre.
pub fn elevation_sensitivity_mrad_per_m(a: &AmmoDef, r: f64) -> f64 {
    let dr = 10.0;
    let th = |x: f64| drop_at(a, x) / x;
    (th(r + dr) - th(r - dr)) / (2.0 * dr) * 1000.0
}

/// Lanz–Odermatt long-rod penetration into RHA at normal incidence (mm).
pub fn long_rod_penetration_mm(p: &PenetrationModel, length_mm: f64, diameter_mm: f64, rho: f64, v: f64) -> f64 {
    let ld = length_mm / diameter_mm;
    let coth = 1.0 / libm::tanh(p.b0.get() + p.b1.get() * ld);
    let bhn = p.target_bhn.get();
    let e = libm::exp(-p.c.get() * bhn * bhn / (rho * v * v));
    length_mm * p.a.get() * coth * libm::sqrt(rho / p.target_density_g_cm3.get()) * e
}

/// Penetration into RHA at normal incidence for a round arriving at speed `v` (mm): long-rod
/// model for APFSDS, the muzzle figure scaled by (v / v0)² for autocannon sabot (kinetic
/// energy), a constant for shaped charges and HE (their effect does not depend on speed).
pub fn penetration_mm(p: &PenetrationModel, a: &AmmoDef, v: f64) -> f64 {
    let flat = || a.flat_penetration_mm.as_ref().map_or(0.0, |x| x.get());
    match a.kind {
        AmmoKind::Apfsds => {
            let (l, d, rho) = a.rod().expect("validated at load");
            long_rod_penetration_mm(p, l, d, rho, v)
        }
        AmmoKind::Apds => {
            let r = v / a.muzzle_velocity_mps.get();
            flat() * r * r
        }
        AmmoKind::Heat | AmmoKind::HeFrag => flat(),
        AmmoKind::Smoke => 0.0,
    }
}

// --- hit resolution -----------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    Front,
    Side,
    Rear,
    /// Turret roof or engine deck: plunging fire or a top-attack missile (plan M8B-AT-TEAMS §3).
    Top,
}

#[derive(Debug, Clone)]
pub struct ShotOutcome {
    pub hit: bool,
    pub turret: bool,
    pub face: Face,
    pub penetration_mm: f64,
    pub armour_mm: f64,
    pub penetrated: bool,
    pub kill: Option<KillKind>,
    pub sights_damaged: bool,
    /// An ERA tile fired (and is spent on impact).
    pub era_fired: bool,
}

/// Inputs describing the shot geometry.
pub struct ShotGeometry {
    pub range: f64,
    /// Visible fraction of the target from the shooter.
    pub visible: f64,
    /// Compass bearing from target to shooter.
    pub bearing_to_shooter: f64,
    pub target_heading: f64,
    /// Target speed perpendicular to the line of fire (m/s).
    pub crossing_speed: f64,
    pub shooter_moving: bool,
    /// Angle of the line of fire below the horizontal at the target (rad; negative when the
    /// shooter is lower).
    pub plunge: f64,
}

/// Vertical angle of the line of fire at the target: from a firing height `from_z` (m above the
/// datum) to the middle of a target whose ground is at `to_z`, `half_h` up, `range` away.
pub fn plunge_angle(from_z: f64, to_z: f64, half_h: f64, range: f64) -> f64 {
    libm::atan2(from_z - (to_z + half_h), range.max(1.0))
}

/// Total 1σ random error (mrad) for one round, excluding per-engagement bias. Public for the
/// validation harness.
pub fn random_sigma_mrad(fc: &FireControlDef, ammo: &AmmoDef, g: &ShotGeometry) -> (f64, f64) {
    let aim = if g.shooter_moving { fc.moving_error_mrad.get() } else { fc.aim_error_mrad.get() };
    let disp = ammo.dispersion_mrad.get();
    // A guided round is steered onto the moving target: no lead error.
    let lead = if ammo.guided {
        0.0
    } else {
        g.crossing_speed * time_of_flight(ammo, g.range) / g.range * 1000.0 * fc.lead_error_fraction.get()
    };
    let v = libm::sqrt(disp * disp + aim * aim);
    let h = libm::sqrt(disp * disp + aim * aim + lead * lead);
    (v, h)
}

/// Per-engagement bias σ (mrad): ranging error through the superelevation plus residual
/// ballistic error.
pub fn bias_sigma_mrad(fc: &FireControlDef, ammo: &AmmoDef, range: f64) -> f64 {
    let range_sigma = if fc.laser_rangefinder {
        fc.laser_range_error_m.get()
    } else {
        fc.estimated_range_error_fraction.get() * range
    };
    let ranging = elevation_sensitivity_mrad_per_m(ammo, range) * range_sigma;
    let b = if ammo.guided { 0.0 } else { fc.ballistic_error_mrad.get() };
    libm::sqrt(ranging * ranging + b * b)
}

/// Damage roll on table `o`. `behind` scales the catastrophic, firepower and crew outcomes (a small
/// warhead's narrow path behind the armour; plan M8B-AT-TEAMS §2.3); mobility keeps its value, and
/// the difference becomes a hit with no lasting effect.
pub(crate) fn roll(o: &Outcomes, exposed_factor: f64, behind: f64, u: f64) -> Option<KillKind> {
    let cat = (o.catastrophic.get() * exposed_factor).min(1.0) * behind;
    let mut acc = cat;
    if u < acc {
        return Some(KillKind::Catastrophic);
    }
    acc += o.firepower.get() * behind;
    if u < acc {
        return Some(KillKind::Firepower);
    }
    acc += o.mobility.get();
    if u < acc {
        return Some(KillKind::Mobility);
    }
    acc += o.crew.get() * behind;
    if u < acc {
        return Some(KillKind::Crew);
    }
    None
}

/// Face and obliquity a round meets: the top for a top-attack missile or a shot plunging more
/// steeply than `top_above` (obliquity from the roof's normal), else by aspect.
fn hit_face(g: &ShotGeometry, ammo: &AmmoDef, top_above: f64) -> (Face, f64) {
    let dive = ammo.top_attack_deg.as_ref().map(|d| d.get().to_radians());
    let plunge = dive.unwrap_or(g.plunge);
    if dive.is_some() || plunge > top_above {
        return (Face::Top, std::f64::consts::FRAC_PI_2 - plunge);
    }
    face_of(g.bearing_to_shooter, g.target_heading)
}

fn face_of(bearing_to_shooter: f64, target_heading: f64) -> (Face, f64) {
    let a = crate::wrap(bearing_to_shooter - target_heading).abs();
    if a < std::f64::consts::FRAC_PI_4 {
        (Face::Front, a)
    } else if a > 3.0 * std::f64::consts::FRAC_PI_4 {
        (Face::Rear, std::f64::consts::PI - a)
    } else {
        (Face::Side, (std::f64::consts::FRAC_PI_2 - a).abs())
    }
}

/// Line-of-sight armour (mm RHA equivalent) the round must defeat. `turret` None averages hull
/// and turret (used when choosing a round before the hit location is known).
fn line_of_sight_armour(target: &VehicleDef, face: Face, obliq: f64, chemical: bool, turret: Option<bool>) -> f64 {
    let ar = &target.armour;
    let (t, h) = match face {
        Face::Front => (ar.turret_front.get(), ar.hull_front.get()),
        Face::Side => (ar.turret_side.get(), ar.hull_side.get()),
        Face::Rear => (ar.turret_rear.get(), ar.hull_rear.get()),
        // Plain steel: no composite advantage against shaped charges.
        Face::Top => {
            let top = ar.top.get();
            return top / libm::cos(obliq.min(70f64.to_radians()));
        }
    };
    let base = match turret {
        Some(true) => t,
        Some(false) => h,
        None => 0.5 * (t + h),
    };
    let ce = if chemical { ar.ce_factor.get() } else { 1.0 };
    base * ce / libm::cos(obliq.min(70f64.to_radians()))
}

/// Resolve one round: error sample → hit test against the visible silhouette → face and
/// obliquity → penetration vs. armour (ERA, KE or CE) → damage roll. `era_left` is the number of
/// reactive tiles the target still carries. The shooter is only its fire control, so a squad's
/// launcher fires through the same path as a vehicle's gun (plan M8B-AT-TEAMS §2.2).
#[allow(clippy::too_many_arguments)]
pub fn resolve(
    fc: &FireControlDef,
    target: &VehicleDef,
    era_left: u32,
    ammo: &AmmoDef,
    pen: &PenetrationModel,
    dmg: &DamageTables,
    g: &ShotGeometry,
    bias: (f64, f64),
    rng: &mut ChaCha8Rng,
) -> ShotOutcome {
    let (sv, sh) = random_sigma_mrad(fc, ammo, g);
    let ev = (bias.0 + gauss(rng) * sv) * g.range / 1000.0;
    let eh = (bias.1 + gauss(rng) * sh) * g.range / 1000.0;
    let d = &target.dimensions;
    let top = d.height_m.get();
    let vis_h = (top * g.visible).max(0.2);
    let width = projected_width(target, g.target_heading, g.bearing_to_shooter);
    let hit = eh.abs() <= width / 2.0 && ev.abs() <= vis_h / 2.0;
    let impact_h = top - vis_h / 2.0 + ev;
    let turret = impact_h > d.hull_height_m.get();
    let (face, obliq) = hit_face(g, ammo, dmg.top_hit_above_deg.get().to_radians());
    let chemical = ammo.kind.is_chemical();
    let mut armour = line_of_sight_armour(target, face, obliq, chemical, Some(turret));
    let mut p = penetration_mm(pen, ammo, velocity_at(ammo, g.range));
    let u_dmg = uniform(rng);
    let u_sight = uniform(rng);
    let u_era = uniform(rng);
    let mut era_fired = false;
    if hit
        && era_left > 0
        && matches!(face, Face::Front | Face::Side)
        && let Some(era) = &target.armour.era
        && u_era < era.coverage.get()
    {
        era_fired = true;
        // A tandem precursor fires the tile ahead of the main jet (plan M8B-AT-TEAMS §2.4).
        if chemical && !ammo.tandem {
            p *= 1.0 - era.ce_defeat.get();
        } else {
            armour += era.ke_gain_mm.get() / libm::cos(obliq.min(70f64.to_radians()));
        }
    }
    if !hit {
        return ShotOutcome {
            hit,
            turret,
            face,
            penetration_mm: p,
            armour_mm: armour,
            penetrated: false,
            kill: None,
            sights_damaged: false,
            era_fired: false,
        };
    }
    let penetrated = p > armour;
    let exposed =
        if target.ammo_stowage == sim_data::AmmoStowage::Exposed { dmg.exposed_stowage_factor.get() } else { 1.0 };
    let table = match (penetrated, turret, face) {
        (false, _, _) => &dmg.non_penetration,
        (true, true, _) => &dmg.turret_penetration,
        (true, false, Face::Front) => &dmg.hull_front_penetration,
        // Hull side, rear and engine deck.
        (true, false, _) => &dmg.hull_side_rear_penetration,
    };
    let (exposed, behind) = if penetrated { (exposed, ammo.behind_armour_factor()) } else { (1.0, 1.0) };
    let kill = roll(table, exposed, behind, u_dmg);
    let sights_damaged = !penetrated && u_sight < dmg.non_penetration_sights.get();
    ShotOutcome { hit, turret, face, penetration_mm: p, armour_mm: armour, penetrated, kill, sights_damaged, era_fired }
}

/// Expected penetration over the armour round `a` meets on `target` from `geo` (hull and turret
/// averaged, ERA weighted by its coverage): above 1 the round is expected to penetrate.
pub fn round_margin(
    a: &AmmoDef,
    pen: &PenetrationModel,
    target: &VehicleDef,
    era_left: u32,
    geo: &ShotGeometry,
    top_above: f64,
) -> f64 {
    let (face, obliq) = hit_face(geo, a, top_above);
    let chem = a.kind.is_chemical();
    let mut p = penetration_mm(pen, a, velocity_at(a, geo.range));
    if chem
        && !a.tandem
        && era_left > 0
        && matches!(face, Face::Front | Face::Side)
        && let Some(era) = &target.armour.era
    {
        p *= 1.0 - era.coverage.get() * era.ce_defeat.get();
    }
    p / line_of_sight_armour(target, face, obliq, chem, None)
}

/// The face a shot from `geo` meets (public for the squad launchers' aspect preference).
pub fn face_for(geo: &ShotGeometry, a: &AmmoDef, top_above: f64) -> Face {
    hit_face(geo, a, top_above).0
}

/// Which round of the loadout the crew fires at a target: the commander's choice if stocked and
/// in range, else a stand-off round beyond its `prefer_beyond` range (per loadout entry; missiles
/// beyond the gun's effective range, plan M8B-AT-TEAMS §5) that defeats the target, else the first
/// round meant for the target's class that defeats its armour (stand-off rounds only beyond their
/// range), else the one with the best margin (FM 3-20.12 ammunition selection by target type).
/// Beyond `open_fire_m` (the doctrine's open-fire range) only stand-off rounds may fire.
#[allow(clippy::too_many_arguments)]
pub fn select_round(
    g: &GunState,
    loadout: &[usize],
    beyond: &[Option<f64>],
    open_fire_m: f64,
    ammo: &[AmmoDef],
    pen: &PenetrationModel,
    target: &VehicleDef,
    era_left: u32,
    geo: &ShotGeometry,
    top_above: f64,
) -> Option<usize> {
    let usable = |li: usize| {
        let a = &ammo[loadout[li]];
        g.stock[li] > 0
            && a.kind != AmmoKind::Smoke
            && a.max_range_m.as_ref().is_none_or(|m| geo.range <= m.get())
            && (geo.range <= open_fire_m || beyond.get(li).copied().flatten().is_some())
    };
    let margin = |li: usize| round_margin(&ammo[loadout[li]], pen, target, era_left, geo, top_above);
    let all: Vec<usize> = (0..loadout.len()).filter(|&li| usable(li)).collect();
    if let Some(kind) = g.selected
        && let Some(&li) = all.iter().find(|&&li| ammo[loadout[li]].kind == kind)
    {
        return Some(li);
    }
    let stand_off = |li: usize| beyond.get(li).copied().flatten();
    if let Some(&li) = all.iter().find(|&&li| stand_off(li).is_some_and(|b| geo.range > b) && margin(li) >= 1.0) {
        return Some(li);
    }
    if let Some(&li) = all.iter().find(|&&li| {
        stand_off(li).is_none_or(|b| geo.range > b)
            && ammo[loadout[li]].preferred_vs.contains(&target.class.into())
            && margin(li) >= 1.0
    }) {
        return Some(li);
    }
    let mut best: Option<(usize, f64)> = None;
    for &li in &all {
        let m = margin(li);
        if best.is_none_or(|(_, bm)| m > bm) {
            best = Some((li, m));
        }
    }
    best.map(|(li, _)| li)
}

/// What a round does when it lands.
#[derive(Debug, Clone)]
pub enum Effect {
    /// Shot at a vehicle; its outcome was sampled at the trigger pull.
    Shot { target: usize, outcome: ShotOutcome },
    /// Smoke shell: `clouds` clouds of smoke kind `kind` at the impact point.
    Smoke { kind: usize, clouds: u32 },
    /// HE fired at infantry: bursts at the impact point (plan M8B-INFANTRY §4).
    Burst { lethal_radius_m: f64 },
}

/// A round in flight.
#[derive(Debug, Clone)]
pub struct Projectile {
    pub impact_tick: u64,
    pub shooter: usize,
    /// Command-guided missile: the squad whose gunner must keep tracking until impact (plan
    /// M8B-AT-TEAMS §2.2, operator in the loop).
    pub operator: Option<usize>,
    /// The operator was pinned or lost during the flight: the missile misses.
    pub guidance_lost: bool,
    pub from: [f64; 2],
    pub to: [f64; 2],
    pub effect: Effect,
}

/// Apply a damage result to the target. Returns the resulting kill kind for reporting.
pub fn apply_damage(world: &mut World, t: usize, kill: Option<KillKind>, sights: bool) -> Option<KillKind> {
    if sights && let Some(s) = world.sensors[t].as_mut() {
        s.degraded = true;
    }
    let c = world.condition[t];
    let new = match (kill, c) {
        (None, _) => return None,
        (_, Condition::Destroyed) => return None,
        (Some(KillKind::Catastrophic), _) => Condition::Destroyed,
        (Some(KillKind::Firepower), Condition::MobilityKill) => Condition::Destroyed,
        (Some(KillKind::Mobility), Condition::FirepowerKill) => Condition::Destroyed,
        (Some(KillKind::Firepower), _) => Condition::FirepowerKill,
        (Some(KillKind::Mobility), _) => Condition::MobilityKill,
        (Some(KillKind::Crew), _) => {
            if let Some(g) = world.gun[t].as_mut() {
                g.crew_loss = g.crew_loss.saturating_add(1);
                if g.crew_loss >= 2 {
                    world.condition[t] = Condition::Destroyed;
                    return Some(KillKind::Catastrophic);
                }
            }
            return Some(KillKind::Crew);
        }
    };
    world.condition[t] = new;
    if new != Condition::Operational {
        if let Some(m) = world.mobility[t].as_mut()
            && new != Condition::FirepowerKill
        {
            m.speed = 0.0;
            m.cmd_speed = 0.0;
        }
        if (new == Condition::Destroyed || new == Condition::FirepowerKill)
            && let Some(g) = world.gun[t].as_mut()
        {
            g.target = None;
            g.phase = Phase::Ready;
        }
    }
    if new == Condition::Destroyed { Some(KillKind::Catastrophic) } else { kill }
}

/// A squad seen launching a rocket or missile stays the first threat this long (ticks, 60 s).
const LAUNCH_THREAT: u64 = 60 * crate::TICK_HZ as u64;

/// Pick a target from the crew's own tracks: assigned target if engageable, else keep the
/// current one, else by threat (ATP 3-20.15 ch. 4, engagement priorities; App. F, react to an
/// ATGM: engage the launch point): a squad seen launching a rocket or missile, or identified as
/// an anti-tank missile team (plan M8B-AT-TEAMS §5), then infantry inside `close_infantry`
/// (anti-tank weapons at close range), then the nearest vehicle, then the nearest infantry.
fn choose_target(
    world: &World,
    squads: &[sim_data::SquadDef],
    k: usize,
    max_range: f64,
    reach: f64,
    close_infantry: f64,
    tick: u64,
) -> Option<usize> {
    let s = world.sensors[k].as_ref()?;
    let g = world.gun[k].as_ref()?;
    let min_level = match world.weapons[k] {
        WeaponsStatus::Hold => return g.assigned.filter(|&t| s.track(t).is_some()),
        WeaponsStatus::Tight => ContactLevel::Identified,
        WeaponsStatus::Free => ContactLevel::Recognised,
    };
    let ok = |t: usize| {
        let limit = if world.squad[t].is_some() { max_range } else { reach };
        s.track(t).is_some_and(|tr| {
            tr.level >= min_level
                && tick.saturating_sub(tr.last_seen_tick) <= 20
                && tr.visible_fraction > 0.0
                && tr.range_m <= limit
        }) && world.alive(t)
            && !world.mounted(t)
    };
    if let Some(a) = g.assigned
        && ok(a)
    {
        return Some(a);
    }
    let squad = |tr: &crate::world::LocalTrack| world.squad[tr.target].is_some();
    let anti_tank = |tr: &crate::world::LocalTrack| {
        let launched = world.launch[tr.target].is_some_and(|(lt, _)| tick.saturating_sub(lt) <= LAUNCH_THREAT);
        let missile_team = tr.level >= ContactLevel::Identified
            && world.squad[tr.target].as_ref().is_some_and(|q| {
                squads[q.def].anti_armour.iter().any(|w| w.class == sim_data::AntiArmourClass::Missile)
            });
        squad(tr) && (launched || missile_team)
    };
    // Keep the current target unless an anti-tank threat has appeared since.
    if let Some(cur) = g.target
        && ok(cur)
        && (s.track(cur).is_some_and(anti_tank) || !s.tracks.iter().any(|tr| ok(tr.target) && anti_tank(tr)))
    {
        return Some(cur);
    }
    let nearest = |pred: &dyn Fn(&crate::world::LocalTrack) -> bool| {
        s.tracks
            .iter()
            .filter(|tr| ok(tr.target) && pred(tr))
            .min_by(|a, b| a.range_m.total_cmp(&b.range_m).then(a.target.cmp(&b.target)))
            .map(|tr| tr.target)
    };
    // Supply trucks come last: they do not shoot back (plan M8C-TAILGATE-TRUCKS §5).
    let truck = |tr: &crate::world::LocalTrack| world.cargo[tr.target].is_some();
    nearest(&anti_tank)
        .or_else(|| nearest(&|tr| squad(tr) && tr.range_m <= close_infantry))
        .or_else(|| nearest(&|tr| !squad(tr) && !truck(tr)))
        .or_else(|| nearest(&squad))
        .or_else(|| nearest(&truck))
}

/// Main-gun round for infantry: the commander's choice if it bursts, else the first stocked round
/// meant for infantry with a lethal radius (FM 3-20.12: HE or multipurpose at troops in the open).
pub fn select_infantry_round(g: &GunState, loadout: &[usize], ammo: &[AmmoDef], range: f64) -> Option<usize> {
    let usable = |li: usize| {
        let a = &ammo[loadout[li]];
        g.stock[li] > 0 && a.lethal_radius_m.is_some() && a.max_range_m.as_ref().is_none_or(|m| range <= m.get())
    };
    if let Some(kind) = g.selected
        && let Some(li) = (0..loadout.len()).find(|&li| usable(li) && ammo[loadout[li]].kind == kind)
    {
        return Some(li);
    }
    (0..loadout.len())
        .find(|&li| usable(li) && ammo[loadout[li]].preferred_vs.contains(&sim_data::TargetClass::Infantry))
}

/// What the gunners need to know about infantry targets.
pub struct InfantryGunnery<'a> {
    /// Per side: infantry inside this range comes before vehicles (doctrine).
    pub close_infantry: &'a [f64],
    /// Squad types (anti-tank missile teams are a first threat).
    pub squads: &'a [sim_data::SquadDef],
    pub map: &'a terrain::BakedMap,
    pub td: &'a sim_data::TerrainData,
}

pub struct FireEvent {
    pub shooter: usize,
    pub projectile: Projectile,
}

/// Advance every gun by one tick. Returns rounds fired this tick.
#[allow(clippy::too_many_arguments)]
pub fn step(
    world: &mut World,
    vehicles: &[VehicleDef],
    loadouts: &[Vec<usize>],
    ammo: &[AmmoDef],
    pen: &PenetrationModel,
    dmg: &DamageTables,
    smoke: &SmokeModel,
    engage_range: &[f64],
    inf: &InfantryGunnery,
    slow_gain: f64,
    tick: u64,
    rng: &mut ChaCha8Rng,
) -> Vec<FireEvent> {
    let mut fired = Vec::new();
    for k in 0..world.len() {
        if !world.alive(k) || world.condition[k] == Condition::FirepowerKill || world.gun[k].is_none() {
            continue;
        }
        let Some(vi) = world.vehicle[k] else { continue };
        let def = &vehicles[vi];
        if world.gun[k].as_ref().is_some_and(|g| g.mission.is_some()) {
            if let Some(f) = smoke_mission(world, k, def, &loadouts[vi], ammo, smoke, tick, rng) {
                fired.push(f);
            }
            continue;
        }
        let side = world.side[k].0 as usize;
        let crew_max = def.armament.max_engagement_range_m.get();
        let max_range = crew_max.min(engage_range[side]);
        let beyond: Vec<Option<f64>> =
            def.armament.load.iter().map(|l| l.prefer_beyond_m.as_ref().map(|b| b.get())).collect();
        // Stand-off rounds (gun-launched and vehicle missiles) open fire at vehicles out to their
        // own reach, past the doctrine's open-fire range for the gun (B6, N9).
        let reach = world.gun[k].as_ref().map_or(max_range, |g| {
            (0..loadouts[vi].len())
                .filter(|&li| beyond[li].is_some() && g.stock[li] > 0)
                .map(|li| ammo[loadouts[vi][li]].max_range_m.as_ref().map_or(crew_max, |m| m.get()).min(crew_max))
                .fold(max_range, f64::max)
        });
        let target = if tick.is_multiple_of(crate::VEHICLE_AI_EVERY)
            || world.gun[k].as_ref().is_some_and(|g| g.target.is_none())
        {
            choose_target(world, inf.squads, k, max_range, reach, inf.close_infantry[side], tick)
        } else {
            world.gun[k].as_ref().and_then(|g| g.target).filter(|&t| world.alive(t) && !world.mounted(t))
        };
        let pos = world.transform[k].pos;
        let heading = world.transform[k].heading;
        let speed = world.mobility[k].as_ref().map_or(0.0, |m| m.speed);
        let slow = (1.0 + 0.5 * world.gun[k].as_ref().map_or(0, |g| g.crew_loss) as f64)
            * (1.0 + slow_gain * world.suppression[k]);
        let traverse = def.armament.turret_traverse_rad_s.get();
        let fc = &def.fire_control;
        let target_info = target.map(|t| {
            let tp = world.transform[t].pos;
            let r = dist(pos, tp);
            let br = libm::atan2(tp[0] - pos[0], tp[1] - pos[1]);
            (t, tp, r, br)
        });
        let u_lay = uniform(rng);
        // Infantry: the coax inside its reach unless the men are behind walls it cannot get
        // through, and nothing better (HE) is loaded; otherwise the main gun with HE.
        let coax = target.filter(|&t| world.squad[t].is_some()).and_then(|t| {
            let c = def.coax.as_ref()?;
            let stocked = world.coax[k].as_ref().is_some_and(|s| s.bursts > 0);
            let r = dist(world.transform[k].pos, world.transform[t].pos);
            let walls = world.squad[t]
                .as_ref()
                .and_then(|s| s.in_building)
                .and_then(|id| inf.map.building(id))
                .is_some_and(|b| inf.td.protection(b.class, sim_data::BuildingThreat::SmallArms) < 0.5);
            let g = world.gun[k].as_ref()?;
            let he = select_infantry_round(g, &loadouts[vi], ammo, r).is_some();
            (stocked && r <= c.max_range_m.get() && !(walls && he)).then_some(t)
        });
        let g = world.gun[k].as_mut().expect("checked");
        g.coax_target = coax;
        if g.target != target {
            g.target = target;
            g.rounds_at_target = 0;
            g.phase = match target_info {
                Some((_, _, _, br)) => {
                    let dturn = crate::wrap(br - g.turret).abs();
                    Phase::Laying { left: (dturn / traverse + fc.lay_time_s.get() * (0.7 + 0.6 * u_lay)) * slow }
                }
                None => Phase::Ready,
            };
        }
        let Some((t, tp, r, br)) = target_info else {
            // Turret returns to the hull's front when idle.
            let d = crate::wrap(heading - g.turret);
            g.turret = crate::wrap(g.turret + d.clamp(-traverse * DT, traverse * DT));
            continue;
        };
        let d = crate::wrap(br - g.turret);
        g.turret = crate::wrap(g.turret + d.clamp(-traverse * DT, traverse * DT));
        g.phase = match g.phase {
            Phase::Laying { left } if left > DT => Phase::Laying { left: left - DT },
            Phase::Laying { .. } => {
                Phase::Lasing { left: if fc.laser_rangefinder { fc.lase_time_s.get() } else { 1.5 } * slow }
            }
            Phase::Lasing { left } if left > DT => Phase::Lasing { left: left - DT },
            Phase::Reloading { left } if left > DT => Phase::Reloading { left: left - DT },
            Phase::Reloading { .. } => Phase::Laying { left: 1.0 * slow }, // re-lay / adjust
            Phase::Lasing { .. } | Phase::Ready => Phase::Ready,
        };
        if g.phase != Phase::Ready || g.rounds() == 0 || g.coax_target.is_some() || tick < g.loading_until {
            continue;
        }
        // Fire.
        let visible = world.sensors[k].as_ref().and_then(|s| s.track(t)).map_or(0.0, |tr| tr.visible_fraction);
        if visible <= 0.0 {
            continue;
        }
        let Some(tvi) = world.vehicle[t] else {
            if let Some(f) = fire_he_at_infantry(world, k, def, &loadouts[vi], ammo, t, r, br, slow, tick, rng) {
                fired.push(f);
            }
            continue;
        };
        let tdef = &vehicles[tvi];
        let tt = world.transform[t];
        let tspeed = world.mobility[t].as_ref().map_or(0.0, |m| m.speed);
        let rel = tt.heading - br;
        let from_z = world.transform[k].z + def.dimensions.sight_height_m.get();
        let geo = ShotGeometry {
            range: r,
            visible,
            bearing_to_shooter: crate::wrap(br + std::f64::consts::PI),
            target_heading: tt.heading,
            crossing_speed: (tspeed * libm::sin(rel)).abs(),
            shooter_moving: speed > 1.0,
            plunge: plunge_angle(from_z, tt.z, 0.5 * tdef.dimensions.height_m.get(), r),
        };
        let era_left = world.era[t];
        let g = world.gun[k].as_mut().expect("checked");
        let top_above = dmg.top_hit_above_deg.get().to_radians();
        let Some(li) = select_round(g, &loadouts[vi], &beyond, max_range, ammo, pen, tdef, era_left, &geo, top_above)
        else {
            continue;
        };
        let a = &ammo[loadouts[vi][li]];
        if g.rounds_at_target == 0 {
            let bs = bias_sigma_mrad(&def.fire_control, a, r);
            g.bias = (gauss(rng) * bs, gauss(rng) * bs * 0.5);
        } else {
            // Adjusted fire: the crew corrects most of the observed bias.
            g.bias = (g.bias.0 * 0.4, g.bias.1 * 0.4);
        }
        let bias = g.bias;
        g.rounds_at_target += 1;
        g.stock[li] -= 1;
        let reload = a.reload_s.as_ref().map_or(def.armament.reload_s.get(), |x| x.get());
        g.phase = Phase::Reloading { left: reload * slow };
        let outcome = resolve(&def.fire_control, tdef, era_left, a, pen, dmg, &geo, bias, rng);
        let tof = time_of_flight(a, r);
        let miss_offset = if outcome.hit { [0.0, 0.0] } else { [gauss(rng) * 6.0, gauss(rng) * 6.0 + 8.0] };
        world.last_fired[k] = Some(tick);
        fired.push(FireEvent {
            shooter: k,
            projectile: Projectile {
                impact_tick: tick + (tof / DT).ceil() as u64,
                shooter: k,
                operator: None,
                guidance_lost: false,
                from: pos,
                to: [tp[0] + miss_offset[0], tp[1] + miss_offset[1]],
                effect: Effect::Shot { target: t, outcome },
            },
        });
    }
    fired
}

/// Main-gun HE at a squad (plan M8B-INFANTRY §4): the round lands at the aim point displaced by
/// the error budget (lateral error as is, vertical error stretched along the line of fire by
/// the flat descent angle) and bursts there.
#[allow(clippy::too_many_arguments)]
fn fire_he_at_infantry(
    world: &mut World,
    k: usize,
    def: &VehicleDef,
    loadout: &[usize],
    ammo: &[AmmoDef],
    t: usize,
    r: f64,
    br: f64,
    slow: f64,
    tick: u64,
    rng: &mut ChaCha8Rng,
) -> Option<FireEvent> {
    let pos = world.transform[k].pos;
    let tp = world.transform[t].pos;
    let speed = world.mobility[k].as_ref().map_or(0.0, |m| m.speed);
    let g = world.gun[k].as_mut().expect("checked");
    let li = select_infantry_round(g, loadout, ammo, r)?;
    let a = &ammo[loadout[li]];
    let geo = ShotGeometry {
        range: r,
        visible: 1.0,
        bearing_to_shooter: crate::wrap(br + std::f64::consts::PI),
        target_heading: 0.0,
        crossing_speed: 0.0,
        shooter_moving: speed > 1.0,
        plunge: 0.0,
    };
    if g.rounds_at_target == 0 {
        let bs = bias_sigma_mrad(&def.fire_control, a, r);
        g.bias = (gauss(rng) * bs, gauss(rng) * bs * 0.5);
    } else {
        g.bias = (g.bias.0 * 0.4, g.bias.1 * 0.4);
    }
    g.rounds_at_target += 1;
    g.stock[li] -= 1;
    let reload = a.reload_s.as_ref().map_or(def.armament.reload_s.get(), |x| x.get());
    g.phase = Phase::Reloading { left: reload * slow };
    let (sv, sh) = random_sigma_mrad(&def.fire_control, a, &geo);
    let ev = (g.bias.0 + gauss(rng) * sv) * r / 1000.0;
    let eh = (g.bias.1 + gauss(rng) * sh) * r / 1000.0;
    // Descent angle of a flat trajectory is about twice the superelevation; at least 0.05 rad.
    let descent = (2.0 * drop_at(a, r) / r).max(0.05);
    let along = ev / descent;
    let (ux, uy) = (libm::sin(br), libm::cos(br));
    let to = [tp[0] + ux * along + uy * eh, tp[1] + uy * along - ux * eh];
    world.last_fired[k] = Some(tick);
    let lethal = a.lethal_radius_m.as_ref().map_or(0.0, |x| x.get());
    Some(FireEvent {
        shooter: k,
        projectile: Projectile {
            impact_tick: tick + (time_of_flight(a, r) / DT).ceil() as u64,
            shooter: k,
            operator: None,
            guidance_lost: false,
            from: pos,
            to,
            effect: Effect::Burst { lethal_radius_m: lethal },
        },
    })
}

/// Fire mission with smoke shells: slew onto the aiming point, fire when the gun is ready.
#[allow(clippy::too_many_arguments)]
fn smoke_mission(
    world: &mut World,
    k: usize,
    def: &VehicleDef,
    loadout: &[usize],
    ammo: &[AmmoDef],
    smoke: &SmokeModel,
    tick: u64,
    rng: &mut ChaCha8Rng,
) -> Option<FireEvent> {
    let pos = world.transform[k].pos;
    let traverse = def.armament.turret_traverse_rad_s.get();
    let slow = 1.0 + 0.5 * world.gun[k].as_ref().map_or(0, |g| g.crew_loss) as f64;
    let g = world.gun[k].as_mut().expect("checked");
    let m = g.mission.expect("checked");
    let Some(li) = (0..loadout.len()).find(|&li| g.stock[li] > 0 && ammo[loadout[li]].kind == AmmoKind::Smoke) else {
        g.mission = None;
        return None;
    };
    g.target = None;
    g.rounds_at_target = 0;
    let br = libm::atan2(m.at[0] - pos[0], m.at[1] - pos[1]);
    let d = crate::wrap(br - g.turret);
    g.turret = crate::wrap(g.turret + d.clamp(-traverse * DT, traverse * DT));
    g.phase = match g.phase {
        Phase::Reloading { left } if left > DT => Phase::Reloading { left: left - DT },
        _ => Phase::Ready,
    };
    if g.phase != Phase::Ready || d.abs() > 0.02 {
        return None;
    }
    let a = &ammo[loadout[li]];
    let kind = a.smoke.as_ref().and_then(|s| smoke.kind(&s.kind).map(|i| (i, s.clouds)))?;
    let r = dist(pos, m.at);
    let scatter = a.dispersion_mrad.get() * r / 1000.0;
    g.stock[li] -= 1;
    g.phase = Phase::Reloading { left: a.reload_s.as_ref().map_or(def.armament.reload_s.get(), |x| x.get()) * slow };
    g.mission = (m.rounds_left > 1).then_some(SmokeMission { at: m.at, rounds_left: m.rounds_left - 1 });
    let to = [m.at[0] + gauss(rng) * scatter, m.at[1] + gauss(rng) * scatter];
    world.last_fired[k] = Some(tick);
    Some(FireEvent {
        shooter: k,
        projectile: Projectile {
            impact_tick: tick + (time_of_flight(a, r) / DT).ceil() as u64,
            shooter: k,
            operator: None,
            guidance_lost: false,
            from: pos,
            to,
            effect: Effect::Smoke { kind: kind.0, clouds: kind.1 },
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ds() -> sim_data::DataSet {
        sim_data::DataSet::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")).unwrap()
    }

    /// Best sabot a side fields in `year` against `target`'s front, as penetration / armour at 2 km
    /// (normal incidence, turret front): the data-level balance check for era pairings.
    fn front_margin(ds: &sim_data::DataSet, shooter: &str, target: &str, year: i32) -> f64 {
        let s = ds.vehicle(shooter).unwrap();
        let best = s
            .armament
            .load
            .iter()
            .map(|l| ds.ammo(&l.ammo).unwrap())
            .filter(|a| a.kind == sim_data::AmmoKind::Apfsds && a.in_service.contains(year))
            .map(|a| penetration_mm(&ds.penetration, a, velocity_at(a, 2000.0)))
            .fold(0.0, f64::max);
        let t = ds.vehicle(target).unwrap();
        // Reactive tiles in service add their KE gain over the share of the front they cover.
        let era = t
            .armour
            .era
            .as_ref()
            .filter(|e| e.in_service.contains(year))
            .map_or(0.0, |e| e.coverage.get() * e.ke_gain_mm.get());
        best / (t.armour.turret_front.get() + era)
    }

    #[test]
    fn cold_war_pairings_are_contestable_and_the_late_eighties_open_a_gap() {
        // 1985 (M1 vs T-72A, the baseline pair): neither tank's sabot is decisive at 2 km. A margin
        // near 1 means a hit penetrates about half the time given the shot geometry.
        let ds = ds();
        let blue = front_margin(&ds, "bulwark_a0", "kurgan_a", 1985);
        let red = front_margin(&ds, "kurgan_a", "bulwark_a0", 1985);
        assert!((0.8..1.3).contains(&blue), "M1 vs T-72A front: {blue:.2}");
        assert!((0.8..1.3).contains(&red), "T-72A vs M1 front: {red:.2}");
        // The 1988+ A1 with the improved sabot outclasses the A, as the historical record shows.
        assert!(front_margin(&ds, "bulwark_a1", "kurgan_a", 1990) > 1.3);
        assert!(front_margin(&ds, "kurgan_a", "bulwark_a1", 1990) < 0.8);
    }

    #[test]
    fn t64_and_t80_classes_are_contestable_in_the_mid_eighties() {
        // 1985 at 2 km, turret front (data only, TODO I1): the M1's 105 mm sabot meets the T-64B
        // about as it meets the T-72A, and the better-armoured T-80B a little worse; their 125 mm
        // sabot of the time (3BM22 class) meets the M1 as the T-72A's does.
        let ds = ds();
        let vs_t72a = front_margin(&ds, "bulwark_a0", "kurgan_a", 1985);
        let vs_t64 = front_margin(&ds, "bulwark_a0", "kholm_b", 1985);
        let vs_t80 = front_margin(&ds, "bulwark_a0", "uval_b", 1985);
        assert!((0.8..1.3).contains(&vs_t64), "M1 vs T-64B front: {vs_t64:.2}");
        assert!((0.8..1.3).contains(&vs_t80), "M1 vs T-80B front: {vs_t80:.2}");
        assert!(vs_t80 < vs_t64 && vs_t64 <= vs_t72a + 0.05, "{vs_t72a:.2} {vs_t64:.2} {vs_t80:.2}");
        for red in ["kholm_b", "uval_b"] {
            let m = front_margin(&ds, red, "bulwark_a0", 1985);
            assert!((0.8..1.3).contains(&m), "{red} vs M1 front: {m:.2}");
        }
        // 1987: the M1A1's early 120 mm sabot still defeats the T-80BV about half the time, while
        // the 3BM42 class does not reliably defeat the A1 (as for the T-72B).
        assert!(front_margin(&ds, "bulwark_a1", "uval_b", 1987) > 0.9);
        assert!(front_margin(&ds, "uval_b", "bulwark_a1", 1987) < 1.0);
    }

    #[test]
    fn modern_pairings_favour_the_better_protected_but_leave_the_t90_a_chance() {
        // About 2005 at 2 km, turret front: the A2's sabot just defeats the T-90's front with its
        // reactive armour (margin near 1), while the T-90's sabot does not defeat the A2's front.
        let ds = ds();
        let blue = front_margin(&ds, "bulwark_a2", "kurgan_c", 2005);
        let red = front_margin(&ds, "kurgan_c", "bulwark_a2", 2005);
        assert!((0.85..1.25).contains(&blue), "M1A2 vs T-90A front: {blue:.2}");
        assert!(red < 0.85, "T-90A vs M1A2 front: {red:.2}");
        // The upgraded T-72B (thermal sight from 2005, ERA from 1988) cannot hold the A2 either.
        assert!(front_margin(&ds, "bulwark_a2", "kurgan_b", 2005) > 1.3);
    }

    #[test]
    fn a_guided_round_has_no_ranging_bias_and_no_lead_error() {
        // A command-guided missile is steered onto the target: superelevation, ballistic bias and
        // lead do not apply (FM 3-20.12 on ATGM engagement: the gunner tracks to impact), while
        // the unguided 30 mm APDS at the same range keeps its ranging bias.
        let ds = ds();
        let ifv = ds.vehicle("skif_m1").unwrap();
        let atgm = ds.ammo("atgm_sk5").unwrap();
        let apds = ds.ammo("apds_30").unwrap();
        assert!(atgm.guided && !apds.guided);
        assert_eq!(drop_at(atgm, 2000.0), 0.0);
        assert_eq!(bias_sigma_mrad(&ifv.fire_control, atgm, 2000.0), 0.0);
        assert!(bias_sigma_mrad(&ifv.fire_control, apds, 1500.0) > 0.0);
        let g = ShotGeometry {
            range: 2000.0,
            visible: 1.0,
            bearing_to_shooter: 0.0,
            target_heading: 0.0,
            crossing_speed: 10.0,
            shooter_moving: false,
            plunge: 0.0,
        };
        let (v, h) = random_sigma_mrad(&ifv.fire_control, atgm, &g);
        assert_eq!(v, h, "no lead error for a guided round");
    }

    #[test]
    fn time_of_flight_matches_closed_form() {
        // Linear decay v = v0 - k r  ⇒  t = ln(v0 / (v0 - k r)) / k (exact integral of dr / v).
        let ds = ds();
        let a = ds.ammo("apfsds_120a").unwrap();
        let t = time_of_flight(a, 2000.0);
        assert!((t - (1575.0f64 / 1455.0).ln() / 0.06).abs() < 1e-9);
        assert!(t > 2000.0 / 1575.0 && t < 2000.0 / 1455.0);
    }

    #[test]
    fn penetration_matches_reference_values_within_ten_percent() {
        let ds = ds();
        for id in [
            "apfsds_120a",
            "apfsds_125b",
            "apfsds_120_early",
            "apfsds_105a",
            "apfsds_105b",
            "apfsds_125a",
            "apfsds_120b",
            "apfsds_125c",
        ] {
            let a = ds.ammo(id).unwrap();
            let p = penetration_mm(&ds.penetration, a, velocity_at(a, 2000.0));
            let reference = a.reference_penetration_2km_mm.as_ref().unwrap().get();
            assert!((p - reference).abs() / reference < 0.10, "{id}: {p:.0} mm vs reference {reference:.0} mm");
        }
    }

    #[test]
    fn penetration_rises_with_velocity_and_length() {
        let ds = ds();
        let a = ds.ammo("apfsds_120a").unwrap().clone();
        let p1 = penetration_mm(&ds.penetration, &a, 1400.0);
        let p2 = penetration_mm(&ds.penetration, &a, 1600.0);
        assert!(p2 > p1);
        let mut longer = a.clone();
        longer.rod_length_mm.as_mut().unwrap().value *= 1.2;
        assert!(penetration_mm(&ds.penetration, &longer, 1500.0) > penetration_mm(&ds.penetration, &a, 1500.0));
    }

    #[test]
    fn hit_probability_falls_with_range_and_visibility() {
        use rand_chacha::rand_core::SeedableRng;
        let ds = ds();
        let s = ds.vehicle("bulwark_a1").unwrap();
        let t = ds.vehicle("kurgan_b").unwrap();
        let a = ds.ammo("apfsds_120a").unwrap();
        let ph = |r: f64, vis: f64| {
            let mut rng = ChaCha8Rng::seed_from_u64(1);
            let geo = ShotGeometry {
                range: r,
                visible: vis,
                bearing_to_shooter: 0.0,
                target_heading: 0.0,
                crossing_speed: 0.0,
                shooter_moving: false,
                plunge: 0.0,
            };
            let n = 4000;
            let hits = (0..n)
                .filter(|_| {
                    let bs = bias_sigma_mrad(&s.fire_control, a, r);
                    let bias = (gauss(&mut rng) * bs, gauss(&mut rng) * bs * 0.5);
                    resolve(&s.fire_control, t, 0, a, &ds.penetration, &ds.damage, &geo, bias, &mut rng).hit
                })
                .count();
            hits as f64 / n as f64
        };
        let (p1, p3, p3h) = (ph(1000.0, 1.0), ph(3000.0, 1.0), ph(3000.0, 0.35));
        assert!(p1 > 0.9, "first-round Ph at 1 km = {p1}");
        assert!(p3 < p1 && p3 > 0.3, "Ph at 3 km = {p3}");
        assert!(p3h < p3, "hull-down target harder to hit: {p3h} vs {p3}");
    }

    /// A side-on shot at 100 m that always hits (geometry for the warhead tests).
    fn side_shot() -> ShotGeometry {
        ShotGeometry {
            range: 100.0,
            visible: 1.0,
            bearing_to_shooter: std::f64::consts::FRAC_PI_2,
            target_heading: 0.0,
            crossing_speed: 0.0,
            shooter_moving: false,
            plunge: 0.0,
        }
    }

    fn perfect_sight() -> FireControlDef {
        let mut fc = ds().vehicle("bulwark_a1").unwrap().fire_control.clone();
        for s in [&mut fc.aim_error_mrad, &mut fc.ballistic_error_mrad, &mut fc.moving_error_mrad] {
            s.value = 0.0;
        }
        fc
    }

    #[test]
    fn a_tandem_rocket_spends_the_tile_without_losing_penetration() {
        // Target A6 (plan M8B-AT-TEAMS §11): PG-7VR against Kontakt-class ERA; the single-charge
        // PG-7VL loses the tile's share of its penetration.
        use rand_chacha::rand_core::SeedableRng;
        let ds = ds();
        let t72b = ds.vehicle("kurgan_b").unwrap();
        let era = t72b.armour.era.as_ref().unwrap();
        let fc = perfect_sight();
        let mut fired = (0, 0);
        for seed in 0..200 {
            for (id, n) in [("rkt_pg7vr", &mut fired.0), ("rkt_pg7vl", &mut fired.1)] {
                let a = ds.ammo(id).unwrap();
                let mut rng = ChaCha8Rng::seed_from_u64(seed);
                let o = resolve(&fc, t72b, 10, a, &ds.penetration, &ds.damage, &side_shot(), (0.0, 0.0), &mut rng);
                assert!(o.hit);
                if o.era_fired {
                    *n += 1;
                    let full = a.flat_penetration_mm.as_ref().unwrap().get();
                    let want = if a.tandem { full } else { full * (1.0 - era.ce_defeat.get()) };
                    assert!((o.penetration_mm - want).abs() < 1e-9, "{id}: {} vs {want}", o.penetration_mm);
                }
            }
        }
        assert!(fired.0 > 100 && fired.1 > 100, "tiles fire on most side hits: {fired:?}");
    }

    #[test]
    fn light_rockets_need_about_three_hits_to_destroy_a_t72_from_the_side() {
        // Target A8 (plan M8B-AT-TEAMS §11): PG-7V class hits on a T-72 side until it is destroyed
        // (catastrophic or firepower kill), mean 2-4 (Grau 1997: 3-6 lethal hits per destroyed
        // vehicle in Grozny, all aspects). Without the behind-armour factor it would be about one.
        use rand_chacha::rand_core::SeedableRng;
        let ds = ds();
        let t72 = ds.vehicle("kurgan_a").unwrap();
        let fc = perfect_sight();
        let mut rng = ChaCha8Rng::seed_from_u64(11);
        let mean_hits = |a: &AmmoDef, rng: &mut ChaCha8Rng| {
            let n = 2000;
            let mut total = 0u32;
            for _ in 0..n {
                let (mut fp, mut crew) = (false, 0);
                for hits in 1..=30 {
                    // Aim at the centre of mass: half the hits on the turret, half on the hull.
                    let bias = (gauss(rng) * 3.0, 0.0);
                    let o = resolve(&fc, t72, 0, a, &ds.penetration, &ds.damage, &side_shot(), bias, rng);
                    match o.kill {
                        // A mobility kill leaves the tank fighting (a later firepower kill destroys it).
                        Some(KillKind::Catastrophic | KillKind::Firepower) => fp = true,
                        Some(KillKind::Crew) => crew += 1,
                        Some(KillKind::Mobility) | None => {}
                    }
                    if fp || crew >= 2 || hits == 30 {
                        total += hits;
                        break;
                    }
                }
            }
            total as f64 / n as f64
        };
        let pg7v = ds.ammo("rkt_pg7v").unwrap().clone();
        let m = mean_hits(&pg7v, &mut rng);
        println!("PG-7V hits to destroy a T-72 from the side: {m:.2}");
        assert!((2.0..=4.0).contains(&m), "mean hits to destroy {m:.2}");
        let mut full = pg7v.clone();
        full.behind_armour = None;
        let one = mean_hits(&full, &mut rng);
        assert!(one < 1.8, "a full-effect penetration kills at once most of the time: {one:.2}");
    }

    #[test]
    fn an_upper_floor_rocket_penetrates_an_mbt_roof_and_not_its_front() {
        // Target A5 and §10.4 (plan M8B-AT-TEAMS): a PG-7V from an upper floor (plunge over 20 deg)
        // meets the roof and penetrates it; the same rocket fired level at the T-72's front
        // does not (target A4: front not penetrated, side penetrated).
        use rand_chacha::rand_core::SeedableRng;
        let ds = ds();
        let t72 = ds.vehicle("kurgan_a").unwrap();
        let a = ds.ammo("rkt_pg7v").unwrap();
        let fc = perfect_sight();
        let shot = |bearing: f64, plunge: f64, seed: u64| {
            let mut g = side_shot();
            g.bearing_to_shooter = bearing;
            g.range = 40.0;
            g.plunge = plunge;
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            resolve(&fc, t72, 0, a, &ds.penetration, &ds.damage, &g, (0.0, 0.0), &mut rng)
        };
        // 12 m up at 25 m: 25 deg.
        let plunge = plunge_angle(100.0 + 12.0, 100.0, 1.1, 25.0);
        assert!(plunge.to_degrees() > 20.0, "{:.1} deg", plunge.to_degrees());
        for seed in 0..20 {
            let roof = shot(0.0, plunge, seed);
            assert!(roof.hit && roof.face == Face::Top && roof.penetrated, "{roof:?}");
            let front = shot(0.0, 0.0, seed);
            assert!(front.face == Face::Front && !front.penetrated, "{front:?}");
            let side = shot(std::f64::consts::FRAC_PI_2, 0.0, seed);
            assert!(side.face == Face::Side && side.penetrated, "{side:?}");
        }
        // A ridge 50 m above a tank 1 km away is not a plunging shot.
        assert!(plunge_angle(150.0, 100.0, 1.1, 1000.0) < ds.damage.top_hit_above_deg.get().to_radians());
    }

    #[test]
    fn a_top_attack_missile_meets_the_roof_whatever_the_aspect() {
        // Javelin class (plan M8B-AT-TEAMS §3): the roof, not the T-90's front with its ERA.
        use rand_chacha::rand_core::SeedableRng;
        let ds = ds();
        let t90 = ds.vehicle("kurgan_c").unwrap();
        let jv = ds.ammo("atgm_javelin").unwrap();
        let fc = perfect_sight();
        let mut g = side_shot();
        g.bearing_to_shooter = 0.0;
        g.range = 1500.0;
        let mut rng = ChaCha8Rng::seed_from_u64(5);
        let o = resolve(&fc, t90, 10, jv, &ds.penetration, &ds.damage, &g, (0.0, 0.0), &mut rng);
        assert_eq!(o.face, Face::Top);
        assert!(o.penetrated && !o.era_fired, "{o:?}");
    }

    #[test]
    fn missile_carriers_keep_their_missiles_for_stand_off() {
        // Plan M8B-AT-TEAMS §5 (B6): the M2 fires TOW at a BMP-2 beyond 2 km and its 25 mm inside
        // it; at a tank it uses the missile at any range (the gun cannot defeat it).
        let ds = ds();
        let m2 = ds.vehicle("ward_m2").unwrap();
        let ammo: Vec<AmmoDef> = m2.armament.load.iter().map(|l| ds.ammo(&l.ammo).unwrap().clone()).collect();
        let loadout: Vec<usize> = (0..ammo.len()).collect();
        let beyond: Vec<Option<f64>> =
            m2.armament.load.iter().map(|l| l.prefer_beyond_m.as_ref().map(|b| b.get())).collect();
        let g = GunState::new(0.0, m2.armament.load.iter().map(|l| l.count).collect());
        let pick = |target: &str, r: f64| {
            let mut geo = side_shot();
            geo.bearing_to_shooter = 0.0;
            geo.range = r;
            let t = ds.vehicle(target).unwrap();
            let li =
                select_round(&g, &loadout, &beyond, f64::INFINITY, &ammo, &ds.penetration, t, 0, &geo, 0.35).unwrap();
            ammo[li].guided
        };
        assert!(pick("skif_m1", 2500.0), "TOW beyond the gun's range");
        assert!(!pick("skif_m1", 1000.0), "25 mm inside it");
        assert!(pick("kurgan_a", 1000.0), "TOW at a tank");
    }

    #[test]
    fn only_stand_off_rounds_fire_past_the_open_fire_range() {
        // N9: a T-64B class tank with a 2.5 km open-fire range fires its gun-launched missile at
        // 3 km, and nothing once the missiles are gone.
        let ds = ds();
        let t64 = ds.vehicle("kholm_b").unwrap();
        let ammo: Vec<AmmoDef> = t64.armament.load.iter().map(|l| ds.ammo(&l.ammo).unwrap().clone()).collect();
        let loadout: Vec<usize> = (0..ammo.len()).collect();
        let beyond: Vec<Option<f64>> =
            t64.armament.load.iter().map(|l| l.prefer_beyond_m.as_ref().map(|b| b.get())).collect();
        let mut g = GunState::new(0.0, t64.armament.load.iter().map(|l| l.count).collect());
        let mut geo = side_shot();
        geo.bearing_to_shooter = 0.0;
        geo.range = 3000.0;
        let t = ds.vehicle("bulwark_a0").unwrap();
        let li = select_round(&g, &loadout, &beyond, 2500.0, &ammo, &ds.penetration, t, 0, &geo, 0.35).unwrap();
        assert!(ammo[li].guided);
        g.stock[li] = 0;
        assert_eq!(select_round(&g, &loadout, &beyond, 2500.0, &ammo, &ds.penetration, t, 0, &geo, 0.35), None);
        geo.range = 2000.0;
        let li = select_round(&g, &loadout, &beyond, 2500.0, &ammo, &ds.penetration, t, 0, &geo, 0.35).unwrap();
        assert!(!ammo[li].guided, "the gun inside the open-fire range");
    }
}
