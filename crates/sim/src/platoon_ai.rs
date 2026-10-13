//! Platoon AI (1 Hz): executes platoon orders with doctrine-pack parameters.
//!
//! - Formation movement: virtual leader on a planned route, slot offsets from the doctrine
//!   pack (ATP 3-20.15 (2025) ch. 3; Pottinger 1999). The platoon slows for laggards.
//! - Movement techniques (ATP 3-20.15 ch. 3): traveling; traveling overwatch (trail section
//!   back by the overwatch distance); bounding overwatch (sections alternate bounds while the
//!   other halts oriented along the route).
//! - Occupy a battle position (ATP 3-20.15 ch. 5): hull-down finder, dispersed positions,
//!   crews report position type when set.
//! - Fire distribution (ATP 3-20.15 ch. 4): frontal fire pattern over reported contacts.
//! - Battle drill (ATP 3-20.15 App. F: react to direct fire contact): with `halt_on_contact`
//!   the platoon halts, orients on the threat and returns fire; it continues the mission once
//!   the contact has been quiet for a while.

use ai::formation::{along, length, slot_position};
use ai::picture::{Condition, PerceivedWorld};
use ai::position::{BpQuery, Candidate, assign, evaluate};
use sim_data::{DoctrinePack, Technique, VehicleDef};
use terrain::BakedMap;
use terrain::ground::Ground;
use terrain::path::Pathfinder;

use crate::command::{PlatoonReport, PositionKind};
use crate::platoon::{Platoon, PlatoonOrder};
use crate::world::{Order, World};
use crate::{DT, Event, Vec2, dist};

/// Formation travel speed as a fraction of the slowest member's cross-country speed: room for
/// vehicles to keep station over rough ground (estimate).
const FORMATION_SPEED_FRACTION: f64 = 0.6;
/// Re-plan a member's route to its slot when the slot moved this far (m) or this often (s).
const REPLAN_DISTANCE: f64 = 25.0;
const REPLAN_EVERY_S: f64 = 4.0;
/// A contact counts as current if reported within this time (s).
const CONTACT_FRESH_S: f64 = 25.0;
/// Resume the mission after this long without fresh contacts (s).
const QUIET_RESUME_S: f64 = 45.0;
const SLOT_ARRIVE: f64 = 6.0;
const BP_ARRIVE: f64 = 1.5;

pub struct Ctx<'a> {
    pub world: &'a mut World,
    pub vehicles: &'a [VehicleDef],
    pub map: &'a BakedMap,
    pub td: &'a sim_data::TerrainData,
    pub pictures: &'a [PerceivedWorld],
    pub doctrines: &'a [DoctrinePack],
    pub pathfinder: &'a mut Pathfinder,
    pub tick: u64,
    pub events: &'a mut Vec<Event>,
}

impl Ctx<'_> {
    fn report(&mut self, p: &Platoon, r: PlatoonReport) {
        self.events.push(Event::PlatoonReport { side: p.side, platoon: p.id, report: r });
    }

    fn route(&mut self, k: usize, to: Vec2) -> Option<Vec<Vec2>> {
        let vi = self.world.vehicle[k]?;
        let layer = self.map.mobility(&self.vehicles[vi].id)?;
        self.pathfinder.find(&layer.speed, self.map.manifest.cell_size_m, self.world.transform[k].pos, to)
    }

    /// Route through checkpoints; also returns the arc length of each checkpoint.
    fn route_via(&mut self, k: usize, via: &[Vec2], to: Vec2) -> Option<(Vec<Vec2>, Vec<f64>)> {
        let vi = self.world.vehicle[k]?;
        let layer = self.map.mobility(&self.vehicles[vi].id)?;
        self.pathfinder.find_via(&layer.speed, self.map.manifest.cell_size_m, self.world.transform[k].pos, via, to)
    }
}

pub fn step(platoons: &mut [Platoon], ctx: &mut Ctx) {
    for p in platoons.iter_mut() {
        let alive = p.alive_members(ctx.world);
        if alive.is_empty() {
            continue;
        }
        let strength = alive.len() as u8;
        if p.last_strength != 0 && strength < p.last_strength {
            let r = PlatoonReport::Strength { remaining: strength, of: p.members.len() as u8 };
            ctx.report(p, r);
        }
        p.last_strength = strength;
        let doctrine = ctx.doctrines[p.side.0 as usize].clone();
        if !matches!(p.order, PlatoonOrder::Move { .. }) {
            p.withdraw = false;
        }
        fire_distribution(p, ctx, &doctrine);
        contact_drill(p, ctx, &doctrine);
        match p.order.clone() {
            PlatoonOrder::Move { .. } => move_step(p, ctx, &doctrine),
            PlatoonOrder::Occupy { .. } => occupy_step(p, ctx, &doctrine),
            PlatoonOrder::Contact => contact_step(p, ctx),
            PlatoonOrder::None => {}
        }
    }
}

fn fresh_contacts(pic: &PerceivedWorld) -> Vec<Vec2> {
    pic.live_contacts().filter(|c| pic.time_s - c.last_report_s < CONTACT_FRESH_S).map(|c| c.pos).collect()
}

fn centroid(world: &World, members: &[usize]) -> Vec2 {
    let n = members.len().max(1) as f64;
    [
        members.iter().map(|&k| world.transform[k].pos[0]).sum::<f64>() / n,
        members.iter().map(|&k| world.transform[k].pos[1]).sum::<f64>() / n,
    ]
}

fn fire_distribution(p: &Platoon, ctx: &mut Ctx, d: &DoctrinePack) {
    let pic = &ctx.pictures[p.side.0 as usize];
    let shooters: Vec<usize> = p
        .members
        .iter()
        .copied()
        .filter(|&k| ctx.world.alive(k) && ctx.world.condition[k] != Condition::FirepowerKill)
        .collect();
    let c = centroid(ctx.world, &shooters);
    let range = d.engagement_range_m.get();
    let targets: Vec<Vec2> = fresh_contacts(pic).into_iter().filter(|t| dist(*t, c) < range).collect();
    if shooters.is_empty() {
        return;
    }
    if targets.is_empty() {
        for &k in &shooters {
            if let Some(g) = ctx.world.gun[k].as_mut() {
                g.assigned = None;
            }
        }
        return;
    }
    let pos: Vec<Vec2> = shooters.iter().map(|&k| ctx.world.transform[k].pos).collect();
    let plan = ai::firedist::frontal(&pos, &targets);
    for (i, &k) in shooters.iter().enumerate() {
        let tpos = targets[plan[i]];
        // The crew lays on its own track closest to the reported contact.
        let track = ctx.world.sensors[k].as_ref().and_then(|s| {
            s.tracks
                .iter()
                .filter(|t| dist(ctx.world.transform[t.target].pos, tpos) < 250.0)
                .min_by(|a, b| {
                    dist(ctx.world.transform[a.target].pos, tpos)
                        .total_cmp(&dist(ctx.world.transform[b.target].pos, tpos))
                })
                .map(|t| t.target)
        });
        if let Some(g) = ctx.world.gun[k].as_mut() {
            g.assigned = track;
        }
    }
}

fn contact_drill(p: &mut Platoon, ctx: &mut Ctx, d: &DoctrinePack) {
    if !d.halt_on_contact || p.withdraw || !matches!(p.order, PlatoonOrder::Move { .. }) {
        return;
    }
    let pic = &ctx.pictures[p.side.0 as usize];
    let alive = p.alive_members(ctx.world);
    let c = centroid(ctx.world, &alive);
    let Some(threat) = fresh_contacts(pic)
        .into_iter()
        .filter(|t| dist(*t, c) < d.engagement_range_m.get())
        .min_by(|a, b| dist(*a, c).total_cmp(&dist(*b, c)))
    else {
        return;
    };
    // Checkpoints already passed are not revisited when the move resumes.
    if let Some((destination, ahead)) = p.order.move_ahead() {
        p.resume = Some((destination, ahead, p.technique));
    }
    p.order = PlatoonOrder::Contact;
    p.quiet_since = ctx.tick;
    for &k in &alive {
        let b = ai::drive::bearing(ctx.world.transform[k].pos, threat);
        if let Some(ai) = ctx.world.ai[k].as_mut() {
            ai.order = Order::Face { heading: b };
        }
    }
    ctx.report(p, PlatoonReport::Contact { at: c, toward: threat });
}

fn contact_step(p: &mut Platoon, ctx: &mut Ctx) {
    let pic = &ctx.pictures[p.side.0 as usize];
    if !fresh_contacts(pic).is_empty()
        || p.alive_members(ctx.world).iter().any(|&k| ctx.world.gun[k].as_ref().is_some_and(|g| g.target.is_some()))
    {
        p.quiet_since = ctx.tick;
        return;
    }
    if (ctx.tick - p.quiet_since) as f64 * DT >= QUIET_RESUME_S {
        match p.resume.take() {
            Some((dest, via, tech)) => {
                p.technique = tech;
                p.order = PlatoonOrder::new_move(dest, via);
                ctx.report(p, PlatoonReport::ContinuingMission);
            }
            None => p.order = PlatoonOrder::None,
        }
    }
}

fn section(i: usize, n: usize) -> usize {
    if i < n.div_ceil(2) { 0 } else { 1 }
}

fn move_step(p: &mut Platoon, ctx: &mut Ctx, d: &DoctrinePack) {
    let movers: Vec<usize> = p.members.iter().copied().filter(|&k| ctx.world.can_move(k)).collect();
    let PlatoonOrder::Move { destination, via, mut checkpoints, mut route, mut progress, mut active } = p.order.clone()
    else {
        return;
    };
    if movers.is_empty() {
        p.order = PlatoonOrder::None;
        return;
    }
    if route.is_empty() {
        let leader = movers[0];
        match ctx.route_via(leader, &via, destination) {
            Some((r, marks)) => {
                route = r;
                checkpoints = marks;
                progress = [0.0, 0.0];
                active = 0;
                p.slot_targets = vec![[f64::NAN, f64::NAN]; p.members.len()];
            }
            None => {
                ctx.report(p, PlatoonReport::NoRoute);
                p.order = PlatoonOrder::None;
                return;
            }
        }
    }
    let total = length(&route);
    let spacing = d.spacing_m.get();
    let slowest = movers
        .iter()
        .filter_map(|&k| ctx.world.vehicle[k])
        .map(|vi| ctx.vehicles[vi].mobility.max_speed_cross_country_mps.get())
        .fold(f64::INFINITY, f64::min);
    // A march speed caps the virtual leader and every vehicle (catching up included).
    let cap = p.speed_limit.unwrap_or(f64::INFINITY);
    let mut v = (slowest * FORMATION_SPEED_FRACTION).min(cap);
    let fdef = d.formation(p.formation).clone();
    let n = p.members.len();
    let slot_of = |i: usize, progress: &[f64; 2]| -> (Vec2, f64) {
        let sec = section(i, n);
        let s = match p.technique {
            Technique::Traveling | Technique::TravelingOverwatch => progress[0],
            Technique::BoundingOverwatch => progress[sec],
        };
        let (lead, h) = along(&route, s);
        let mut off = fdef.slots.get(i).copied().unwrap_or((0.0, -(i as f64)));
        off = (off.0 * spacing, off.1 * spacing);
        if p.technique == Technique::TravelingOverwatch && sec == 1 {
            off.1 -= d.overwatch_distance_m.get();
        }
        if p.technique == Technique::BoundingOverwatch {
            // Each section keeps its own formation around its own virtual leader.
            let base = fdef.slots.get(if sec == 0 { 0 } else { n.div_ceil(2) }).copied().unwrap_or((0.0, 0.0));
            off = (off.0 - base.0 * spacing, off.1 - base.1 * spacing);
        }
        (slot_position(lead, h, off), h)
    };
    // Laggards slow the platoon down.
    let mut max_lag: f64 = 0.0;
    for (i, &k) in p.members.iter().enumerate() {
        if ctx.world.can_move(k) {
            let (sp, _) = slot_of(i, &progress);
            max_lag = max_lag.max(dist(ctx.world.transform[k].pos, sp));
        }
    }
    if max_lag > 4.0 * spacing {
        v = 0.0;
    } else if max_lag > 2.0 * spacing {
        v *= 0.3;
    }
    let dt = crate::PLATOON_AI_EVERY as f64 * DT;
    match p.technique {
        Technique::Traveling | Technique::TravelingOverwatch => {
            progress[0] = (progress[0] + v * dt).min(total);
            progress[1] = progress[0];
        }
        Technique::BoundingOverwatch => {
            let other = 1 - active;
            let cap = (progress[other] + d.bound_length_m.get()).min(total);
            progress[active] = (progress[active] + v * 1.3 * dt).min(cap);
            let settled = max_lag < 3.0 * SLOT_ARRIVE;
            if progress[active] >= cap && settled && !(progress[0] >= total && progress[1] >= total) {
                active = other;
                ctx.report(p, PlatoonReport::Bounding { section: active as u8 + 1 });
            }
        }
    }
    // Route members to their slots.
    let replan_due = (ctx.tick.saturating_sub(p.replanned)) as f64 * DT >= REPLAN_EVERY_S;
    if replan_due {
        p.replanned = ctx.tick;
    }
    if p.slot_targets.len() != n {
        p.slot_targets = vec![[f64::NAN, f64::NAN]; n];
    }
    for (i, &k) in p.members.iter().enumerate() {
        if !ctx.world.can_move(k) {
            continue;
        }
        let (sp, h) = slot_of(i, &progress);
        let stationary = p.technique == Technique::BoundingOverwatch && section(i, n) != active;
        let far = dist(ctx.world.transform[k].pos, sp) > SLOT_ARRIVE;
        // NaN (never routed) counts as moved.
        let d_slot = dist(p.slot_targets[i], sp);
        let moved = d_slot.is_nan() || d_slot >= REPLAN_DISTANCE;
        if far && (moved || replan_due) {
            if let Some(r) = ctx.route(k, sp) {
                let limit = ctx.vehicles[ctx.world.vehicle[k].expect("vehicle")]
                    .mobility
                    .max_speed_road_mps
                    .get()
                    .min(v.max(slowest * 0.5) * 1.6)
                    .min(cap);
                ctx.world.set_route(k, r, Some(limit), Some(h), true, SLOT_ARRIVE);
            }
            p.slot_targets[i] = sp;
        } else if !far && (stationary || progress[0] >= total) {
            // Halted in overwatch or at the end: orient along the route.
            if let Some(ai) = ctx.world.ai[k].as_mut()
                && matches!(ai.order, Order::Idle)
            {
                ai.order = Order::Face { heading: h };
            }
        }
    }
    p.heading = along(&route, progress[0]).1;
    let done = progress[0] >= total && progress[1] >= total && max_lag < SLOT_ARRIVE * 2.0;
    if done {
        let c = centroid(ctx.world, &movers);
        p.order = PlatoonOrder::None;
        ctx.report(p, PlatoonReport::SetAt { pos: c });
    } else {
        p.order = PlatoonOrder::Move { destination, via, checkpoints, route, progress, active };
    }
}

fn occupy_step(p: &mut Platoon, ctx: &mut Ctx, d: &DoctrinePack) {
    let PlatoonOrder::Occupy { centre, radius, facing, kind, assigned, reported } = p.order.clone() else { return };
    let movers: Vec<usize> = p.members.iter().copied().filter(|&k| ctx.world.can_move(k)).collect();
    if !assigned {
        let Some(vi) = movers.first().and_then(|&k| ctx.world.vehicle[k]) else { return };
        let v = &ctx.vehicles[vi];
        let q = BpQuery {
            centre,
            radius,
            facing,
            hull_height: v.dimensions.hull_height_m.get(),
            sight_height: v.dimensions.sight_height_m.get(),
            gun_depression: -v.gun.depression_rad.get(),
            turret_down: kind == PositionKind::TurretDown,
        };
        let g = Ground::new(ctx.map, ctx.td);
        let layer = ctx.map.mobility(&v.id).expect("layer");
        let hf = &ctx.map.elevation;
        let passable = |x: f64, y: f64| {
            let (i, j) = hf.cell_of(x, y);
            layer.speed.get(i, j) > 0 && hf.node_slope(i, j) < 0.3
        };
        let cands = evaluate(&g, &q, &passable);
        let starts: Vec<Vec2> = movers.iter().map(|&k| ctx.world.transform[k].pos).collect();
        let mut chosen = assign(&cands, movers.len(), d.bp_dispersion_m.get(), &starts);
        if chosen.is_empty() {
            // No usable ground found: line up across the centre facing the threat.
            chosen = (0..movers.len())
                .map(|i| Candidate {
                    pos: slot_position(centre, facing, ((i as f64 - (movers.len() as f64 - 1.0) / 2.0) * 80.0, 0.0)),
                    score: 0.0,
                    hull_hidden: false,
                    coverage: 1.0,
                })
                .collect();
        }
        p.bp.clear();
        for (&k, c) in movers.iter().zip(chosen) {
            if let Some(r) = ctx.route(k, c.pos) {
                ctx.world.set_route(k, r, None, Some(facing), true, BP_ARRIVE);
            }
            p.bp.push((k, c));
        }
        p.order = PlatoonOrder::Occupy { centre, radius, facing, kind, assigned: true, reported: false };
        return;
    }
    if reported {
        return;
    }
    let all_set = p.bp.iter().all(|(k, _)| !ctx.world.can_move(*k) || ctx.world.ai[*k].as_ref().is_some_and(|a| a.set));
    if all_set {
        let (mut hd, mut td, mut ex) = (0u8, 0u8, 0u8);
        for (k, c) in &p.bp {
            match c.note() {
                "HULL-DOWN" => hd += 1,
                "TURRET-DOWN" => td += 1,
                _ => ex += 1,
            }
            if let Some(ai) = ctx.world.ai[*k].as_mut() {
                ai.position_note = Some(c.note().to_owned());
            }
        }
        p.order = PlatoonOrder::Occupy { centre, radius, facing, kind, assigned: true, reported: true };
        ctx.report(p, PlatoonReport::SetInPosition { hull_down: hd, turret_down: td, exposed: ex, facing });
    }
}
