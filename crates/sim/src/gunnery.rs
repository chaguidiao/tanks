//! Gunnery: target selection, fire-control timeline, hit probability, terminal effects
//! (DESIGN.md §7.2–7.4).
//!
//! Timeline (TC 3-20.31 engagement sequence): acquire → traverse/lay → lase → fire →
//! observe → reload → re-engage. Hits come from an error budget sampled per round against the
//! target's *visible* silhouette, so hull-down targets are harder to hit. Penetration is the
//! Lanz–Odermatt long-rod form; armour is per face with line-of-sight obliquity.

use ai::picture::{Condition, ContactLevel, WeaponsStatus};
use rand_chacha::ChaCha8Rng;
use sim_data::{AmmoDef, AmmoKind, DamageTables, Outcomes, PenetrationModel, SmokeModel, VehicleDef};
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
            stock,
            selected: None,
            mission: None,
            target: None,
            assigned: None,
            phase: Phase::Ready,
            rounds_at_target: 0,
            bias: (0.0, 0.0),
            crew_loss: 0,
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
}

/// Total 1σ random error (mrad) for one round, excluding per-engagement bias. Public for the
/// validation harness.
pub fn random_sigma_mrad(def: &VehicleDef, ammo: &AmmoDef, g: &ShotGeometry) -> (f64, f64) {
    let fc = &def.fire_control;
    let aim = if g.shooter_moving { fc.moving_error_mrad.get() } else { fc.aim_error_mrad.get() };
    let disp = ammo.dispersion_mrad.get();
    let lead = g.crossing_speed * time_of_flight(ammo, g.range) / g.range * 1000.0 * fc.lead_error_fraction.get();
    let v = libm::sqrt(disp * disp + aim * aim);
    let h = libm::sqrt(disp * disp + aim * aim + lead * lead);
    (v, h)
}

/// Per-engagement bias σ (mrad): ranging error through the superelevation plus residual
/// ballistic error.
pub fn bias_sigma_mrad(def: &VehicleDef, ammo: &AmmoDef, range: f64) -> f64 {
    let fc = &def.fire_control;
    let range_sigma = if fc.laser_rangefinder {
        fc.laser_range_error_m.get()
    } else {
        fc.estimated_range_error_fraction.get() * range
    };
    let ranging = elevation_sensitivity_mrad_per_m(ammo, range) * range_sigma;
    let b = fc.ballistic_error_mrad.get();
    libm::sqrt(ranging * ranging + b * b)
}

fn roll(o: &Outcomes, exposed_factor: f64, u: f64) -> Option<KillKind> {
    let cat = (o.catastrophic.get() * exposed_factor).min(1.0);
    let mut acc = cat;
    if u < acc {
        return Some(KillKind::Catastrophic);
    }
    acc += o.firepower.get();
    if u < acc {
        return Some(KillKind::Firepower);
    }
    acc += o.mobility.get();
    if u < acc {
        return Some(KillKind::Mobility);
    }
    acc += o.crew.get();
    if u < acc {
        return Some(KillKind::Crew);
    }
    None
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
/// reactive tiles the target still carries.
#[allow(clippy::too_many_arguments)]
pub fn resolve(
    shooter: &VehicleDef,
    target: &VehicleDef,
    era_left: u32,
    ammo: &AmmoDef,
    pen: &PenetrationModel,
    dmg: &DamageTables,
    g: &ShotGeometry,
    bias: (f64, f64),
    rng: &mut ChaCha8Rng,
) -> ShotOutcome {
    let (sv, sh) = random_sigma_mrad(shooter, ammo, g);
    let ev = (bias.0 + gauss(rng) * sv) * g.range / 1000.0;
    let eh = (bias.1 + gauss(rng) * sh) * g.range / 1000.0;
    let d = &target.dimensions;
    let top = d.height_m.get();
    let vis_h = (top * g.visible).max(0.2);
    let width = projected_width(target, g.target_heading, g.bearing_to_shooter);
    let hit = eh.abs() <= width / 2.0 && ev.abs() <= vis_h / 2.0;
    let impact_h = top - vis_h / 2.0 + ev;
    let turret = impact_h > d.hull_height_m.get();
    let (face, obliq) = face_of(g.bearing_to_shooter, g.target_heading);
    let chemical = ammo.kind.is_chemical();
    let mut armour = line_of_sight_armour(target, face, obliq, chemical, Some(turret));
    let mut p = penetration_mm(pen, ammo, velocity_at(ammo, g.range));
    let u_dmg = uniform(rng);
    let u_sight = uniform(rng);
    let u_era = uniform(rng);
    let mut era_fired = false;
    if hit
        && era_left > 0
        && face != Face::Rear
        && let Some(era) = &target.armour.era
        && u_era < era.coverage.get()
    {
        era_fired = true;
        if chemical {
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
        (true, false, _) => &dmg.hull_side_rear_penetration,
    };
    let kill = roll(table, if penetrated { exposed } else { 1.0 }, u_dmg);
    let sights_damaged = !penetrated && u_sight < dmg.non_penetration_sights.get();
    ShotOutcome { hit, turret, face, penetration_mm: p, armour_mm: armour, penetrated, kill, sights_damaged, era_fired }
}

/// Which round of the loadout the crew fires at a target: the commander's choice if stocked and
/// in range, else the first round meant for the target's class that defeats its armour, else the
/// one with the best margin (FM 3-20.12 ammunition selection by target type).
#[allow(clippy::too_many_arguments)]
pub fn select_round(
    g: &GunState,
    loadout: &[usize],
    ammo: &[AmmoDef],
    pen: &PenetrationModel,
    target: &VehicleDef,
    era_left: u32,
    geo: &ShotGeometry,
) -> Option<usize> {
    let (face, obliq) = face_of(geo.bearing_to_shooter, geo.target_heading);
    let usable = |li: usize| {
        let a = &ammo[loadout[li]];
        g.stock[li] > 0 && a.kind != AmmoKind::Smoke && a.max_range_m.as_ref().is_none_or(|m| geo.range <= m.get())
    };
    let margin = |li: usize| {
        let a = &ammo[loadout[li]];
        let chem = a.kind.is_chemical();
        let mut p = penetration_mm(pen, a, velocity_at(a, geo.range));
        if chem
            && era_left > 0
            && face != Face::Rear
            && let Some(era) = &target.armour.era
        {
            p *= 1.0 - era.coverage.get() * era.ce_defeat.get();
        }
        p / line_of_sight_armour(target, face, obliq, chem, None)
    };
    let all: Vec<usize> = (0..loadout.len()).filter(|&li| usable(li)).collect();
    if let Some(kind) = g.selected
        && let Some(&li) = all.iter().find(|&&li| ammo[loadout[li]].kind == kind)
    {
        return Some(li);
    }
    if let Some(&li) =
        all.iter().find(|&&li| ammo[loadout[li]].preferred_vs.contains(&target.class) && margin(li) >= 1.0)
    {
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
}

/// A round in flight.
#[derive(Debug, Clone)]
pub struct Projectile {
    pub impact_tick: u64,
    pub shooter: usize,
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

/// Pick a target from the crew's own tracks: assigned target if engageable, else keep the
/// current one, else the nearest engageable enemy.
fn choose_target(world: &World, k: usize, max_range: f64, tick: u64) -> Option<usize> {
    let s = world.sensors[k].as_ref()?;
    let g = world.gun[k].as_ref()?;
    let min_level = match world.weapons[k] {
        WeaponsStatus::Hold => return g.assigned.filter(|&t| s.track(t).is_some()),
        WeaponsStatus::Tight => ContactLevel::Identified,
        WeaponsStatus::Free => ContactLevel::Recognised,
    };
    let ok = |t: usize| {
        s.track(t).is_some_and(|tr| {
            tr.level >= min_level
                && tick.saturating_sub(tr.last_seen_tick) <= 20
                && tr.visible_fraction > 0.0
                && tr.range_m <= max_range
        }) && world.alive(t)
    };
    if let Some(a) = g.assigned
        && ok(a)
    {
        return Some(a);
    }
    if let Some(cur) = g.target
        && ok(cur)
    {
        return Some(cur);
    }
    s.tracks
        .iter()
        .filter(|tr| ok(tr.target))
        .min_by(|a, b| a.range_m.total_cmp(&b.range_m).then(a.target.cmp(&b.target)))
        .map(|tr| tr.target)
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
        let max_range = def.armament.max_engagement_range_m.get().min(engage_range[world.side[k].0 as usize]);
        let target = if tick.is_multiple_of(crate::VEHICLE_AI_EVERY)
            || world.gun[k].as_ref().is_some_and(|g| g.target.is_none())
        {
            choose_target(world, k, max_range, tick)
        } else {
            world.gun[k].as_ref().and_then(|g| g.target).filter(|&t| world.alive(t))
        };
        let pos = world.transform[k].pos;
        let heading = world.transform[k].heading;
        let speed = world.mobility[k].as_ref().map_or(0.0, |m| m.speed);
        let slow = 1.0 + 0.5 * world.gun[k].as_ref().map_or(0, |g| g.crew_loss) as f64;
        let traverse = def.armament.turret_traverse_rad_s.get();
        let fc = &def.fire_control;
        let target_info = target.map(|t| {
            let tp = world.transform[t].pos;
            let r = dist(pos, tp);
            let br = libm::atan2(tp[0] - pos[0], tp[1] - pos[1]);
            (t, tp, r, br)
        });
        let u_lay = uniform(rng);
        let g = world.gun[k].as_mut().expect("checked");
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
        if g.phase != Phase::Ready || g.rounds() == 0 {
            continue;
        }
        // Fire.
        let visible = world.sensors[k].as_ref().and_then(|s| s.track(t)).map_or(0.0, |tr| tr.visible_fraction);
        if visible <= 0.0 {
            continue;
        }
        let tdef = &vehicles[world.vehicle[t].expect("vehicle")];
        let tt = world.transform[t];
        let tspeed = world.mobility[t].as_ref().map_or(0.0, |m| m.speed);
        let rel = tt.heading - br;
        let geo = ShotGeometry {
            range: r,
            visible,
            bearing_to_shooter: crate::wrap(br + std::f64::consts::PI),
            target_heading: tt.heading,
            crossing_speed: (tspeed * libm::sin(rel)).abs(),
            shooter_moving: speed > 1.0,
        };
        let era_left = world.era[t];
        let g = world.gun[k].as_mut().expect("checked");
        let Some(li) = select_round(g, &loadouts[vi], ammo, pen, tdef, era_left, &geo) else { continue };
        let a = &ammo[loadouts[vi][li]];
        if g.rounds_at_target == 0 {
            let bs = bias_sigma_mrad(def, a, r);
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
        let outcome = resolve(def, tdef, era_left, a, pen, dmg, &geo, bias, rng);
        let tof = time_of_flight(a, r);
        let miss_offset = if outcome.hit { [0.0, 0.0] } else { [gauss(rng) * 6.0, gauss(rng) * 6.0 + 8.0] };
        world.last_fired[k] = Some(tick);
        fired.push(FireEvent {
            shooter: k,
            projectile: Projectile {
                impact_tick: tick + (tof / DT).ceil() as u64,
                shooter: k,
                from: pos,
                to: [tp[0] + miss_offset[0], tp[1] + miss_offset[1]],
                effect: Effect::Shot { target: t, outcome },
            },
        });
    }
    fired
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
        for id in ["apfsds_120a", "apfsds_125b"] {
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
            };
            let n = 4000;
            let hits = (0..n)
                .filter(|_| {
                    let bs = bias_sigma_mrad(s, a, r);
                    let bias = (gauss(&mut rng) * bs, gauss(&mut rng) * bs * 0.5);
                    resolve(s, t, 0, a, &ds.penetration, &ds.damage, &geo, bias, &mut rng).hit
                })
                .count();
            hits as f64 / n as f64
        };
        let (p1, p3, p3h) = (ph(1000.0, 1.0), ph(3000.0, 1.0), ph(3000.0, 0.35));
        assert!(p1 > 0.9, "first-round Ph at 1 km = {p1}");
        assert!(p3 < p1 && p3 > 0.3, "Ph at 3 km = {p3}");
        assert!(p3h < p3, "hull-down target harder to hit: {p3h} vs {p3}");
    }
}
