//! Fire against infantry (docs/plans/M8B-INFANTRY.md §4): small arms and machine guns resolved
//! as bursts, one draw each, and HE bursts over a squad's spread. Vehicles are only suppressed by
//! small arms; armour-piercing fire stays in `gunnery.rs`.
//!
//! - Burst: expected casualties `λ = rounds × P(round hits a man) × visible fraction × lethality
//!   × building protection`, sampled as a Poisson count; `P(round)` from the circular normal
//!   (target area over the error ellipse, `InfantryModel::round_hit`). FM 3-22.68 (2003) for
//!   rates and burst lengths.
//! - HE: each man inside the squad's spread is hit with the probability that he lies within the
//!   lethal radius (circle overlap over the spread), scaled by posture exposure and building
//!   protection.

use ai::picture::{Condition, ContactLevel, WeaponsStatus};
use rand_chacha::ChaCha8Rng;
use sim_data::{BuildingThreat, InfantryModel, SmallArmDef, SquadDef, TerrainData, VehicleDef};
use terrain::BakedMap;

use crate::sensors::uniform;
use crate::world::{Posture, World};
use crate::{DT, Event, Vec2, dist};

pub struct Ctx<'a> {
    pub world: &'a mut World,
    pub vehicles: &'a [VehicleDef],
    pub squads: &'a [SquadDef],
    pub map: &'a BakedMap,
    pub td: &'a TerrainData,
    pub model: &'a InfantryModel,
    /// Per side: squad strength fraction below which it is combat-ineffective.
    pub ineffective: &'a [f64],
    /// Suppression slows aimed fire: intervals × (1 + slow_gain · s).
    pub slow_gain: f64,
    pub tick: u64,
    pub rng: &'a mut ChaCha8Rng,
    pub events: &'a mut Vec<Event>,
}

/// A volley fired this tick, for the pictures (shot lines, the contact drill's "under fire").
pub struct Volley {
    pub shooter: usize,
    pub from: Vec2,
    pub to: Vec2,
}

/// Poisson-distributed count with mean `lambda` (Knuth), capped.
pub fn poisson(lambda: f64, cap: u8, rng: &mut ChaCha8Rng) -> u8 {
    if lambda <= 0.0 {
        return 0;
    }
    let l = libm::exp(-lambda);
    let (mut k, mut p) = (0u8, 1.0);
    loop {
        p *= uniform(rng);
        if p <= l || k >= cap {
            return k.min(cap);
        }
        k += 1;
    }
}

/// Area of the intersection of two circles of radii `a` and `b` whose centres are `d` apart.
pub fn circle_overlap(a: f64, b: f64, d: f64) -> f64 {
    use std::f64::consts::PI;
    if d >= a + b {
        return 0.0;
    }
    if d <= (a - b).abs() {
        let r = a.min(b);
        return PI * r * r;
    }
    let (a2, b2, d2) = (a * a, b * b, d * d);
    let alpha = libm::acos(((d2 + a2 - b2) / (2.0 * d * a)).clamp(-1.0, 1.0));
    let beta = libm::acos(((d2 + b2 - a2) / (2.0 * d * b)).clamp(-1.0, 1.0));
    a2 * alpha + b2 * beta - 0.5 * libm::sqrt(((-d + a + b) * (d + a - b) * (d - a + b) * (d + a + b)).max(0.0))
}

/// Expected casualties of one volley (plan §4): `rounds × P(round hits a man) × visible fraction
/// × lethality × building protection`.
pub fn burst_lambda(
    model: &InfantryModel,
    rounds: f64,
    man_area: f64,
    sigma_mrad: f64,
    r: f64,
    visible: f64,
    prot: f64,
) -> f64 {
    rounds * model.round_hit(man_area, sigma_mrad, r) * visible * model.lethality_per_hit.get() * prot
}

/// Building protection for squad `k` against `threat` (1 outside buildings).
fn protection(ctx_map: &BakedMap, td: &TerrainData, world: &World, k: usize, threat: BuildingThreat) -> f64 {
    world.squad[k]
        .as_ref()
        .and_then(|s| s.in_building)
        .and_then(|id| ctx_map.building(id))
        .map_or(1.0, |b| td.protection(b.class, threat))
}

/// Man area of squad `k` in its current posture (plan §3 signature rows).
fn man_area(squads: &[SquadDef], map: &BakedMap, td: &TerrainData, world: &World, k: usize) -> f64 {
    let s = world.squad[k].as_ref().expect("squad");
    crate::infantry::signature(&squads[s.def], s, map, td).0.man_area_m2.get()
}

/// Choose a squad's target from its own tracks: the nearest enemy squad in reach of its
/// longest-ranged weapon, else the nearest vehicle inside `vehicle_engage_m` (WEAPONS TIGHT needs
/// identification, HOLD fires at nothing).
fn squad_target(world: &World, k: usize, reach: f64, vehicle_reach: f64, tick: u64) -> Option<usize> {
    let min_level = match world.weapons[k] {
        WeaponsStatus::Hold => return None,
        WeaponsStatus::Tight => ContactLevel::Identified,
        WeaponsStatus::Free => ContactLevel::Recognised,
    };
    let s = world.sensors[k].as_ref()?;
    let fresh = |t: &&crate::world::LocalTrack| {
        t.level >= min_level
            && tick.saturating_sub(t.last_seen_tick) <= 20
            && t.visible_fraction > 0.0
            && world.alive(t.target)
            && !world.mounted(t.target)
    };
    let nearest = |pred: &dyn Fn(usize) -> bool, r: f64| {
        s.tracks
            .iter()
            .filter(fresh)
            .filter(|t| t.range_m <= r && pred(t.target))
            .min_by(|a, b| a.range_m.total_cmp(&b.range_m).then(a.target.cmp(&b.target)))
            .map(|t| t.target)
    };
    nearest(&|t| world.squad[t].is_some(), reach).or_else(|| nearest(&|t| world.vehicle[t].is_some(), vehicle_reach))
}

/// One tick of small-arms fire: squad weapons and vehicle coax.
pub fn step(ctx: &mut Ctx) -> Vec<Volley> {
    let mut out = Vec::new();
    for k in 0..ctx.world.len() {
        if !ctx.world.alive(k) || ctx.world.mounted(k) {
            continue;
        }
        let sup = ctx.world.suppression[k];
        let slow = 1.0 + ctx.slow_gain * sup;
        // Squad weapons.
        let squads = ctx.squads;
        if let Some(s) = ctx.world.squad[k].as_ref() {
            let def = &squads[s.def];
            let reach = s
                .weapons
                .iter()
                .filter(|w| w.bursts > 0)
                .map(|w| def.weapons[w.def].max_range_m.get())
                .fold(0.0, f64::max);
            let target = squad_target(ctx.world, k, reach, ctx.model.vehicle_engage_m.get(), ctx.tick);
            if let Some(s) = ctx.world.squad[k].as_mut() {
                s.target = target;
            }
            let n = ctx.world.squad[k].as_ref().map_or(0, |s| s.weapons.len());
            for wi in 0..n {
                let s = ctx.world.squad[k].as_mut().expect("squad");
                let (strength, men) = (s.strength, s.men);
                let w = &mut s.weapons[wi];
                w.cooldown = (w.cooldown - DT).max(0.0);
                let Some(t) = target else { continue };
                let wd = &def.weapons[w.def];
                let r = dist(ctx.world.transform[k].pos, ctx.world.transform[t].pos);
                if w.cooldown > 0.0 || w.bursts == 0 || r > wd.max_range_m.get() {
                    continue;
                }
                // Weapons still manned: the type's count scaled by the men left.
                let guns = ((w.count as u32 * strength as u32).div_ceil(men.max(1) as u32)).min(w.bursts) as u8;
                if guns == 0 {
                    continue;
                }
                w.bursts -= guns as u32;
                w.cooldown = wd.burst_interval_s.get() * slow;
                let moving = s.speed > 0.1;
                if let Some(v) = fire(ctx, k, t, wd, guns, moving) {
                    out.push(v);
                }
            }
            continue;
        }
        // Coax: fires at the infantry target the gunner laid on (gunnery sets `coax_target`).
        let Some(vi) = ctx.world.vehicle[k] else { continue };
        if ctx.world.condition[k] == Condition::FirepowerKill {
            continue;
        }
        let vehicles = ctx.vehicles;
        let Some(cd) = vehicles[vi].coax.as_ref() else { continue };
        let target = ctx.world.gun[k].as_ref().and_then(|g| g.coax_target).filter(|&t| ctx.world.alive(t));
        let moving = ctx.world.mobility[k].as_ref().is_some_and(|m| m.speed > 1.0);
        let Some(c) = ctx.world.coax[k].as_mut() else { continue };
        c.cooldown = (c.cooldown - DT).max(0.0);
        let Some(t) = target else { continue };
        if c.cooldown > 0.0 || c.bursts == 0 {
            continue;
        }
        c.bursts -= 1;
        c.cooldown = cd.burst_interval_s.get() * slow;
        if let Some(v) = fire(ctx, k, t, cd, 1, moving) {
            out.push(v);
        }
    }
    out
}

/// Resolve `guns` bursts of weapon `wd` from `k` at `t`.
fn fire(ctx: &mut Ctx, k: usize, t: usize, wd: &SmallArmDef, guns: u8, moving: bool) -> Option<Volley> {
    let from = ctx.world.transform[k].pos;
    let to = ctx.world.transform[t].pos;
    let r = dist(from, to).max(1.0);
    let visible = ctx.world.sensors[k].as_ref().and_then(|s| s.track(t)).map_or(0.0, |tr| tr.visible_fraction);
    if visible <= 0.0 {
        return None;
    }
    ctx.world.last_fired[k] = Some(ctx.tick);
    let rounds = wd.rounds_per_burst.get() * guns as f64;
    let mut casualties = 0;
    if ctx.world.squad[t].is_some() {
        // A suppressed or moving firer aims worse.
        let sigma = wd.dispersion_mrad.get() * (1.0 + ctx.world.suppression[k]) * if moving { 2.0 } else { 1.0 };
        let area = man_area(ctx.squads, ctx.map, ctx.td, ctx.world, t);
        let prot = protection(ctx.map, ctx.td, ctx.world, t, BuildingThreat::SmallArms);
        let lambda = burst_lambda(ctx.model, rounds, area, sigma, r, visible, prot);
        let strength = ctx.world.squad[t].as_ref().map_or(0, |s| s.strength);
        casualties = poisson(lambda, strength, ctx.rng);
        let s = &mut ctx.world.suppression[t];
        *s = (*s + ctx.model.suppression_per_burst.get() * (rounds / 10.0).min(1.0) * prot.max(0.5)).min(1.0);
        let thr = ctx.ineffective[ctx.world.side[t].0 as usize];
        crate::infantry::casualties(ctx.world, t, casualties, Some(k), thr, ctx.events);
    } else {
        let s = &mut ctx.world.suppression[t];
        *s = (*s + ctx.model.vehicle_suppression_per_burst.get()).min(1.0);
    }
    ctx.events.push(Event::Volley { unit: ctx.world.id[k], from, to, casualties });
    Some(Volley { shooter: k, from, to })
}

/// HE burst at `at` with lethal radius `lethal_m` against every squad on the ground nearby (any
/// side: HE does not choose).
#[allow(clippy::too_many_arguments)]
pub fn he_burst(
    world: &mut World,
    map: &BakedMap,
    td: &TerrainData,
    model: &InfantryModel,
    ineffective: &[f64],
    at: Vec2,
    lethal_m: f64,
    by: Option<usize>,
    rng: &mut ChaCha8Rng,
    events: &mut Vec<Event>,
) {
    let spread = model.spread_radius_m.get();
    let on_building = {
        let (i, j) = map.elevation.cell_of(at[0], at[1]);
        map.building_id.get(i, j)
    };
    for k in 0..world.len() {
        let Some(s) = world.squad[k].as_ref() else { continue };
        if !world.alive(k) || s.mounted_in.is_some() {
            continue;
        }
        let d = dist(world.transform[k].pos, at);
        // The burst makes everyone near it go to ground, hurt or not.
        let shock = spread + 2.0 * lethal_m;
        if d < shock {
            let sup = &mut world.suppression[k];
            *sup = (*sup + 2.0 * model.suppression_per_burst.get() * (1.0 - d / shock)).min(1.0);
        }
        if d > spread + lethal_m {
            continue;
        }
        // Occupants of a building are inside its walls: fragments from outside, blast from a hit
        // on the building itself.
        let (frac, prot) = match s.in_building.and_then(|id| map.building(id).map(|b| (id, b))) {
            Some((id, b)) => {
                let threat = if on_building == id { BuildingThreat::Blast } else { BuildingThreat::Fragments };
                let reach = libm::sqrt(b.footprint_m2 as f64 / std::f64::consts::PI);
                (
                    circle_overlap(reach, lethal_m, d) / (std::f64::consts::PI * reach * reach),
                    td.protection(b.class, threat),
                )
            }
            None => (circle_overlap(spread, lethal_m, d) / (std::f64::consts::PI * spread * spread), 1.0),
        };
        // In a building the walls are the protection (not also the dug-in exposure).
        let exposure = match s.posture {
            _ if s.in_building.is_some() => 1.0,
            Posture::Moving => model.he_exposure.moving.get(),
            Posture::Halted => model.he_exposure.halted.get(),
            Posture::DugIn => model.he_exposure.dug_in.get(),
        };
        let p = (frac * exposure * prot).clamp(0.0, 1.0);
        let lost = (0..s.strength).filter(|_| uniform(rng) < p).count() as u8;
        let thr = ineffective[world.side[k].0 as usize];
        crate::infantry::casualties(world, k, lost, by, thr, events);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_chacha::rand_core::SeedableRng;

    #[test]
    fn circle_overlap_matches_the_limits_and_a_half_lens() {
        use std::f64::consts::PI;
        assert_eq!(circle_overlap(10.0, 5.0, 20.0), 0.0);
        assert!((circle_overlap(10.0, 5.0, 2.0) - PI * 25.0).abs() < 1e-9, "small circle inside");
        // Two unit circles one radius apart: 2π/3 − √3/2.
        let want = 2.0 * PI / 3.0 - libm::sqrt(3.0) / 2.0;
        assert!((circle_overlap(1.0, 1.0, 1.0) - want).abs() < 1e-9);
    }

    fn model() -> InfantryModel {
        sim_data::DataSet::load(&std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data"))
            .unwrap()
            .infantry
    }

    #[test]
    fn burst_casualties_follow_posture_and_cover() {
        // Hand-computed: a PK burst of 8 rounds at 300 m, σ = 3.5 mrad → σ at the target 1.05 m.
        // Standing man 0.75 m²: P(round) = 0.75 / (2π · 1.05²) = 0.1083; λ = 8 · 0.1083 · 0.6.
        let m = model();
        let s2 = std::f64::consts::TAU * 1.05 * 1.05;
        let open = burst_lambda(&m, 8.0, 0.75, 3.5, 300.0, 1.0, 1.0);
        assert!((open - 8.0 * 0.75 / s2 * 0.6).abs() < 1e-9, "{open}");
        // Prone (0.25 m²): a third of that.
        let prone = burst_lambda(&m, 8.0, 0.25, 3.5, 300.0, 1.0, 1.0);
        assert!((prone / open - 1.0 / 3.0).abs() < 1e-9);
        // In a masonry house: window-sized target (0.2 m²) behind walls passing 25 %.
        let house = burst_lambda(&m, 8.0, 0.2, 3.5, 300.0, 1.0, 0.25);
        assert!((house - 8.0 * 0.2 / s2 * 0.6 * 0.25).abs() < 1e-9);
        assert!(open / house > 14.0, "the house cuts casualties by more than an order of magnitude");
    }

    #[test]
    fn poisson_counts_have_the_right_mean() {
        let mut rng = ChaCha8Rng::seed_from_u64(3);
        let n = 20000;
        let mean = (0..n).map(|_| poisson(1.7, 50, &mut rng) as f64).sum::<f64>() / n as f64;
        assert!((mean - 1.7).abs() < 0.05, "{mean}");
        assert_eq!(poisson(5.0, 2, &mut rng).max(2), 2, "capped at the men left");
    }
}
