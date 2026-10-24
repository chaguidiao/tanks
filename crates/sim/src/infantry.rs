//! Dismounted infantry (docs/plans/M8B-INFANTRY.md; DESIGN.md §14 #29): squads riding in and
//! leaving their carriers, losses, and what happens to the men when the carrier is hit.

use ai::picture::Condition;
use rand_chacha::ChaCha8Rng;
use sim_data::InfantryModel;

use crate::Event;
use crate::command::KillKind;
use crate::sensors::uniform;
use crate::world::{Posture, World};

/// Mounted squads ride with their carrier: same position and heading (plan §2).
pub fn follow_carriers(world: &mut World) {
    for k in 0..world.len() {
        let Some(c) = world.squad[k].as_ref().and_then(|s| s.mounted_in) else { continue };
        world.transform[k] = world.transform[c];
    }
}

/// Take `lost` men off squad `k`. Below the doctrine's threshold the squad is combat-ineffective;
/// with no men left it is destroyed.
pub fn casualties(
    world: &mut World,
    k: usize,
    lost: u8,
    by: Option<usize>,
    ineffective_fraction: f64,
    events: &mut Vec<Event>,
) {
    let Some(s) = world.squad[k].as_mut() else { return };
    let lost = lost.min(s.strength);
    if lost == 0 {
        return;
    }
    s.strength -= lost;
    if (s.strength as f64) < ineffective_fraction * s.men as f64 {
        s.ineffective = true;
    }
    let left = s.strength;
    if left == 0 {
        world.condition[k] = Condition::Destroyed;
        s.target = None;
        s.speed = 0.0;
        if let Some(c) = s.mounted_in.take()
            && let Some(cs) = world.carrier[c].as_mut()
        {
            cs.embarked.retain(|&q| q != k);
        }
    }
    events.push(Event::Casualties { unit: world.id[k], by: by.map(|b| world.id[b]), lost, left });
}

/// Squads in a carrier share its fate (plan §2): a penetration costs a fraction of the men, a
/// catastrophic kill the data's (larger) fraction. If the carrier is destroyed, the survivors
/// bail out beside it.
#[allow(clippy::too_many_arguments)]
pub fn carrier_hit(
    world: &mut World,
    carrier: usize,
    penetrated: bool,
    kill: Option<KillKind>,
    model: &InfantryModel,
    ineffective: &[f64],
    by: Option<usize>,
    rng: &mut ChaCha8Rng,
    events: &mut Vec<Event>,
) {
    let Some(cs) = world.carrier[carrier].as_ref() else { return };
    if cs.embarked.is_empty() || !penetrated {
        return;
    }
    let destroyed = world.condition[carrier] == Condition::Destroyed;
    let p = if destroyed || kill == Some(KillKind::Catastrophic) {
        model.mounted_loss.catastrophic.get()
    } else {
        model.mounted_loss.penetration.get()
    };
    for k in cs.embarked.clone() {
        let Some(s) = world.squad[k].as_ref() else { continue };
        let lost = (0..s.strength).filter(|_| uniform(rng) < p).count() as u8;
        let thr = ineffective[world.side[k].0 as usize];
        casualties(world, k, lost, by, thr, events);
        if destroyed {
            bail_out(world, carrier, k);
        }
    }
    if destroyed && let Some(cs) = world.carrier[carrier].as_mut() {
        cs.embarked.clear();
        cs.dismounting = None;
    }
}

/// Put a squad on the ground behind its carrier (the ramp or doors are at the rear).
pub fn bail_out(world: &mut World, carrier: usize, k: usize) {
    let t = world.transform[carrier];
    let Some(s) = world.squad[k].as_mut() else { return };
    s.mounted_in = None;
    s.posture = Posture::Halted;
    s.halted_s = 0.0;
    s.boarding = None;
    let (sh, ch) = (libm::sin(t.heading), libm::cos(t.heading));
    world.transform[k].pos = [t.pos[0] - sh * 6.0, t.pos[1] - ch * 6.0];
}

// --- movement, posture, mount and dismount -------------------------------------------------

/// Arrival tolerance of a squad on its route (m).
const FOOT_ARRIVE_M: f64 = 2.0;
/// Speed assumed inside a no-go cell (leaving a wall cell after bailing out) (m/s).
const FOOT_ESCAPE_MPS: f64 = 0.3;
/// A squad boards when this close to its carrier's ramp (m).
const BOARD_M: f64 = 8.0;
/// A carrier counts as stopped for the ramp drill below this speed (m/s).
const STOPPED_MPS: f64 = 0.5;
/// Ramp and doors are this far behind the carrier's centre (m).
const RAMP_M: f64 = 6.0;
/// React to contact (ATP 3-21.8 (2016) battle drill 2): a squad moving in the open goes to ground
/// and seeks cover when suppression passes this.
const REACT_SUPPRESSION: f64 = 0.3;
/// After this long quiet in cover, a squad that reacted to contact continues its move (s).
const REACT_QUIET_S: f64 = 30.0;

pub struct Ctx<'a> {
    pub world: &'a mut World,
    pub squads: &'a [sim_data::SquadDef],
    pub vehicles: &'a [sim_data::VehicleDef],
    pub map: &'a terrain::BakedMap,
    pub td: &'a sim_data::TerrainData,
    pub model: &'a InfantryModel,
    pub doctrines: &'a [sim_data::DoctrinePack],
    pub pathfinder: &'a mut terrain::path::Pathfinder,
    pub tick: u64,
    pub events: &'a mut Vec<Event>,
}

fn foot_layer(map: &terrain::BakedMap) -> Option<&terrain::bake::MobilityLayer> {
    map.mobility(terrain::mobility::FOOT_CLASS)
}

/// Foot route on the foot raster (buildings entered only through entrances, TODO H9).
pub fn foot_route(
    map: &terrain::BakedMap,
    pf: &mut terrain::path::Pathfinder,
    from: crate::Vec2,
    via: &[crate::Vec2],
    to: crate::Vec2,
) -> Option<Vec<crate::Vec2>> {
    let layer = foot_layer(map)?;
    pf.find_via(&layer.speed, map.manifest.cell_size_m, from, via, to).map(|r| r.0)
}

fn ramp_of(world: &World, c: usize) -> crate::Vec2 {
    let t = &world.transform[c];
    [t.pos[0] - libm::sin(t.heading) * RAMP_M, t.pos[1] - libm::cos(t.heading) * RAMP_M]
}

/// Walk squads along their routes (every tick): the planning speed of the foot raster, none
/// while pinned (plan §6: follow a foot route).
pub fn move_squads(world: &mut World, map: &terrain::BakedMap, model: &InfantryModel, events: &mut Vec<Event>) {
    let Some(layer) = foot_layer(map) else { return };
    let cell = map.manifest.cell_size_m;
    let pinned = model.pinned_above.get();
    for k in 0..world.len() {
        let Some(s) = world.squad[k].as_ref() else { continue };
        if s.mounted_in.is_some() || !world.alive(k) {
            continue;
        }
        let suppressed = world.suppression[k] > pinned;
        let Some(ai) = world.ai[k].as_mut() else { continue };
        let crate::world::Order::Move { route, segment, speed_limit, face, platoon_slot, arrive } = &mut ai.order
        else {
            if let Some(s) = world.squad[k].as_mut() {
                s.speed = 0.0;
            }
            continue;
        };
        let t = &mut world.transform[k];
        let (i, j) = (libm::floor(t.pos[0] / cell) as i64, libm::floor(t.pos[1] / cell) as i64);
        let q = if layer.speed.in_bounds(i, j) { layer.speed.get(i as usize, j as usize) } else { 0 };
        let mut v = if q == 0 { FOOT_ESCAPE_MPS } else { q as f64 * terrain::mobility::SPEED_QUANTUM };
        if let Some(l) = speed_limit {
            v = v.min(*l);
        }
        if suppressed {
            v = 0.0;
        }
        let mut left = v * crate::DT;
        let tol = arrive.max(FOOT_ARRIVE_M);
        let mut arrived = route.len() < 2 || *segment + 1 >= route.len();
        while left > 0.0 && !arrived {
            let next = route[*segment + 1];
            let d = crate::dist(t.pos, next);
            if d <= left {
                t.pos = next;
                left -= d;
                *segment += 1;
                arrived = *segment + 1 >= route.len();
            } else {
                t.heading = ai::drive::bearing(t.pos, next);
                t.pos = [t.pos[0] + (next[0] - t.pos[0]) / d * left, t.pos[1] + (next[1] - t.pos[1]) / d * left];
                left = 0.0;
            }
        }
        if !arrived && *segment + 2 >= route.len() && route.last().is_some_and(|e| crate::dist(t.pos, *e) <= tol) {
            arrived = true;
        }
        let moved = v > 0.0 && !arrived;
        let (face, slot) = (*face, *platoon_slot);
        if arrived {
            if let Some(h) = face {
                t.heading = h;
            }
            ai.order = crate::world::Order::Idle;
            ai.set = true;
            if !slot {
                events.push(Event::Arrived { unit: world.id[k] });
            }
        }
        if let Some(s) = world.squad[k].as_mut() {
            s.speed = if moved { v } else { 0.0 };
        }
    }
}

/// Squad AI and carrier drills (5 Hz, after the vehicle AI so a carrier in a ramp drill is held).
pub fn step(ctx: &mut Ctx) {
    let dt = crate::VEHICLE_AI_EVERY as f64 * crate::DT;
    carriers(ctx, dt);
    for k in 0..ctx.world.len() {
        let Some(s) = ctx.world.squad[k].as_ref() else { continue };
        if s.mounted_in.is_some() || !ctx.world.alive(k) {
            continue;
        }
        boarding(ctx, k, dt);
        posture(ctx, k, dt);
        react_to_contact(ctx, k);
    }
}

/// Posture (plan §0): moving, halted (kneeling/prone), dug in after the dig-in time or when
/// occupying a building.
fn posture(ctx: &mut Ctx, k: usize, dt: f64) {
    let w = &mut *ctx.world;
    let Some(s) = w.squad[k].as_mut() else { return };
    let dig = ctx.squads[s.def].dig_in_s.get();
    if s.speed > 0.1 {
        s.posture = Posture::Moving;
        s.halted_s = 0.0;
        s.in_building = None;
    } else {
        s.halted_s += dt;
        s.posture = if s.in_building.is_some() || s.halted_s >= dig { Posture::DugIn } else { Posture::Halted };
    }
}

/// ATP 3-21.8 (2016) battle drill 2, "React to contact": a squad moving under fire goes to ground,
/// returns fire and moves to the nearest cover within the doctrine's search distance; once quiet
/// in cover it continues the interrupted move.
fn react_to_contact(ctx: &mut Ctx, k: usize) {
    let sup = ctx.world.suppression[k];
    let Some(s) = ctx.world.squad[k].as_ref() else { return };
    let (in_building, halted_s, ineffective) = (s.in_building.is_some(), s.halted_s, s.ineffective);
    let Some(ai) = ctx.world.ai[k].as_ref() else { return };
    let moving = matches!(ai.order, crate::world::Order::Move { .. });
    let held = ai.held.is_some();
    if in_building {
        return;
    }
    if moving && sup >= REACT_SUPPRESSION && !held {
        let pos = ctx.world.transform[k].pos;
        let reach = ctx.doctrines[ctx.world.side[k].0 as usize].infantry.cover_search_m.get();
        let cover =
            cover_near(ctx.map, ctx.td, pos, reach).and_then(|c| foot_route(ctx.map, ctx.pathfinder, pos, &[], c));
        let ai = ctx.world.ai[k].as_mut().expect("checked");
        ai.held = Some(std::mem::replace(&mut ai.order, crate::world::Order::Idle));
        if let Some(route) = cover {
            ai.order = crate::world::Order::Move {
                route,
                segment: 0,
                speed_limit: None,
                face: None,
                platoon_slot: true,
                arrive: FOOT_ARRIVE_M,
            };
        }
        return;
    }
    // Quiet again in cover: continue the interrupted move.
    if !moving && held && !ineffective && sup < 0.05 && halted_s >= REACT_QUIET_S {
        ctx.world.resume(k);
    }
}

/// Nearest point with cover from view and fire within `reach`: forest, scrub, village gardens or a
/// building's open cell, on passable foot ground.
pub fn cover_near(
    map: &terrain::BakedMap,
    td: &sim_data::TerrainData,
    pos: crate::Vec2,
    reach: f64,
) -> Option<crate::Vec2> {
    let foot = foot_layer(map)?;
    let cell = map.manifest.cell_size_m;
    let (ci, cj) = map.elevation.cell_of(pos[0], pos[1]);
    let r = (reach / cell).ceil() as i64;
    let mut best: Option<(i64, crate::Vec2)> = None;
    for dj in -r..=r {
        for di in -r..=r {
            let d2 = di * di + dj * dj;
            if (d2 as f64) * cell * cell > reach * reach || best.is_some_and(|b| d2 >= b.0) {
                continue;
            }
            let (i, j) = (ci as i64 + di, cj as i64 + dj);
            if !foot.speed.in_bounds(i, j) {
                continue;
            }
            let (i, j) = (i as usize, j as usize);
            if foot.speed.get(i, j) == 0 {
                continue;
            }
            let lc = td.landcover(map.landcover.get(i, j));
            let covered = map.building_id.get(i, j) != 0 || lc.canopy_height_m.get() >= 1.0;
            if covered {
                best = Some((d2, [(i as f64 + 0.5) * cell, (j as f64 + 0.5) * cell]));
            }
        }
    }
    best.map(|b| b.1)
}

/// Ramp drills: a carrier dismounting is held stopped; when the drill time has passed with it
/// stopped, its squads leave by the ramp and walk to security positions around it (plan §6,
/// ATP 3-21.71 (2010) ch. 3: dismount and establish local security).
fn carriers(ctx: &mut Ctx, dt: f64) {
    let mut finished: Vec<usize> = Vec::new();
    for c in 0..ctx.world.len() {
        let Some(left) = ctx.world.carrier[c].as_ref().and_then(|cs| cs.dismounting) else { continue };
        if !ctx.world.alive(c) {
            continue;
        }
        let speed = ctx.world.mobility[c].as_ref().map_or(0.0, |m| m.speed);
        if let Some(m) = ctx.world.mobility[c].as_mut() {
            m.cmd_speed = 0.0;
        }
        if speed > STOPPED_MPS {
            continue;
        }
        let left = left - dt;
        if left > 0.0 {
            if let Some(cs) = ctx.world.carrier[c].as_mut() {
                cs.dismounting = Some(left);
            }
            continue;
        }
        let squads = ctx.world.carrier[c].as_mut().map(|cs| {
            cs.dismounting = None;
            std::mem::take(&mut cs.embarked)
        });
        let squads = squads.unwrap_or_default();
        let radius = ctx.doctrines[ctx.world.side[c].0 as usize].infantry.security_radius_m.get();
        let n = squads.len() as f64;
        let heading = ctx.world.transform[c].heading;
        let centre = ctx.world.transform[c].pos;
        for (i, &k) in squads.iter().enumerate() {
            bail_out(ctx.world, c, k);
            // Security positions on the rear arc, spread ±45° for more than one squad.
            let a = heading + std::f64::consts::PI + if n > 1.0 { (i as f64 / (n - 1.0) - 0.5) * 1.57 } else { 0.0 };
            let at = [centre[0] + libm::sin(a) * radius, centre[1] + libm::cos(a) * radius];
            let from = ctx.world.transform[k].pos;
            if let Some(route) = foot_route(ctx.map, ctx.pathfinder, from, &[], at) {
                ctx.world.set_route(k, route, None, Some(a), true, FOOT_ARRIVE_M);
            }
        }
        ctx.events.push(Event::Dismounted { unit: ctx.world.id[c] });
        finished.push(c);
    }
    // The carrier platoon reports once its last carrier is empty.
    let mut reported = Vec::new();
    for c in finished {
        let Some(pid) = ctx.world.platoon[c] else { continue };
        let busy = (0..ctx.world.len()).any(|q| {
            ctx.world.platoon[q] == Some(pid)
                && ctx.world.carrier[q].as_ref().is_some_and(|cs| cs.dismounting.is_some())
        });
        if !busy && !reported.contains(&pid) {
            reported.push(pid);
            let side = ctx.world.side[c];
            ctx.events.push(Event::PlatoonReport {
                side,
                platoon: pid,
                report: crate::command::PlatoonReport::Dismounted,
            });
        }
    }
}

/// A squad ordered to mount walks to its carrier's ramp and boards once there with the carrier
/// stopped (the mount drill time from the carrier's data).
fn boarding(ctx: &mut Ctx, k: usize, dt: f64) {
    let Some(s) = ctx.world.squad[k].as_ref() else { return };
    let Some(c) = s.carrier else { return };
    let Some(left) = s.boarding else { return };
    if !ctx.world.alive(c) {
        if let Some(s) = ctx.world.squad[k].as_mut() {
            s.boarding = None;
        }
        return;
    }
    let ramp = ramp_of(ctx.world, c);
    let pos = ctx.world.transform[k].pos;
    let speed = ctx.world.mobility[c].as_ref().map_or(0.0, |m| m.speed);
    if crate::dist(pos, ramp) > BOARD_M || speed > STOPPED_MPS {
        // Still walking (or the carrier moved): keep heading for the ramp.
        let idle = ctx.world.ai[k].as_ref().is_some_and(|a| a.order == crate::world::Order::Idle);
        let far = match ctx.world.ai[k].as_ref().map(|a| &a.order) {
            Some(crate::world::Order::Move { route, .. }) => route.last().is_none_or(|e| crate::dist(*e, ramp) > 10.0),
            _ => true,
        };
        if (idle || far)
            && ctx.tick.is_multiple_of(crate::PLATOON_AI_EVERY)
            && let Some(route) = foot_route(ctx.map, ctx.pathfinder, pos, &[], ramp)
        {
            ctx.world.set_route(k, route, None, None, true, FOOT_ARRIVE_M);
        }
        let t = mount_time(ctx.vehicles, ctx.world, c);
        if let Some(s) = ctx.world.squad[k].as_mut() {
            s.boarding = Some(t);
        }
        return;
    }
    if let Some(ai) = ctx.world.ai[k].as_mut() {
        ai.order = crate::world::Order::Idle;
    }
    let left = left - dt;
    if left > 0.0 {
        if let Some(s) = ctx.world.squad[k].as_mut() {
            s.boarding = Some(left);
        }
        return;
    }
    let room = ctx.world.carrier[c].as_ref().is_some_and(|cs| {
        let aboard: u32 =
            cs.embarked.iter().filter_map(|&q| ctx.world.squad[q].as_ref()).map(|q| q.strength as u32).sum();
        aboard + s.strength as u32 <= cs.capacity_men as u32
    });
    let s = ctx.world.squad[k].as_mut().expect("squad");
    s.boarding = None;
    if !room {
        return;
    }
    s.mounted_in = Some(c);
    s.in_building = None;
    s.speed = 0.0;
    s.target = None;
    if let Some(cs) = ctx.world.carrier[c].as_mut() {
        cs.embarked.push(k);
    }
    if let Some(ai) = ctx.world.ai[k].as_mut() {
        ai.held = None;
        ai.set = false;
    }
    ctx.world.transform[k] = ctx.world.transform[c];
    ctx.events.push(Event::Mounted { unit: ctx.world.id[k] });
    // The dismount platoon reports once every squad still fighting is aboard.
    if let Some(pid) = ctx.world.platoon[k] {
        let all = (0..ctx.world.len())
            .filter(|&q| ctx.world.platoon[q] == Some(pid) && ctx.world.alive(q))
            .all(|q| ctx.world.mounted(q));
        if all {
            let side = ctx.world.side[k];
            ctx.events.push(Event::PlatoonReport {
                side,
                platoon: pid,
                report: crate::command::PlatoonReport::Mounted,
            });
        }
    }
}

fn mount_time(vehicles: &[sim_data::VehicleDef], world: &World, c: usize) -> f64 {
    world.vehicle[c].and_then(|vi| vehicles[vi].troops.as_ref()).map_or(30.0, |t| t.mount_s.get())
}

/// Start the ramp drill on every carrier of the given members that has squads aboard.
pub fn order_dismount(world: &mut World, vehicles: &[sim_data::VehicleDef], carriers: &[usize]) -> bool {
    let mut any = false;
    for &c in carriers {
        if !world.alive(c) {
            continue;
        }
        let t = world.vehicle[c].and_then(|vi| vehicles[vi].troops.as_ref()).map(|t| t.dismount_s.get());
        if let (Some(t), Some(cs)) = (t, world.carrier[c].as_mut())
            && !cs.embarked.is_empty()
            && cs.dismounting.is_none()
        {
            cs.dismounting = Some(t);
            any = true;
        }
    }
    if any {
        for &c in carriers {
            world.halt(c);
        }
    }
    any
}

/// Send squads back to their carriers to mount.
pub fn order_mount(world: &mut World, vehicles: &[sim_data::VehicleDef], squads: &[usize]) -> Vec<usize> {
    let mut carriers = Vec::new();
    for &k in squads {
        let Some(s) = world.squad[k].as_ref() else { continue };
        if s.mounted_in.is_some() || !world.alive(k) {
            continue;
        }
        let Some(c) = s.carrier.filter(|&c| world.alive(c)) else { continue };
        let t = mount_time(vehicles, world, c);
        if let Some(s) = world.squad[k].as_mut() {
            s.boarding = Some(t);
            s.in_building = None;
        }
        if let Some(ai) = world.ai[k].as_mut() {
            ai.held = None;
        }
        if !carriers.contains(&c) {
            carriers.push(c);
        }
    }
    for &c in &carriers {
        world.halt(c);
    }
    carriers
}

// --- dismount platoons ---------------------------------------------------------------------

/// Dismount platoon orders (1 Hz): a move puts the squads on line across the direction of travel,
/// `squad_spacing_m` apart, each on its own foot route (ATP 3-21.8 (2016) ch. 2, platoon line);
/// a battle position is a move to its centre. Arrival is reported once every squad is there.
pub fn platoon_step(platoons: &mut [crate::platoon::Platoon], ctx: &mut Ctx) {
    use crate::platoon::PlatoonOrder;
    for p in platoons.iter_mut().filter(|p| p.dismount) {
        if let PlatoonOrder::Occupy { centre, .. } = p.order {
            p.order = PlatoonOrder::new_move(centre, Vec::new());
        }
        let PlatoonOrder::Move { destination, via, route, .. } = &mut p.order else { continue };
        let movers: Vec<usize> = p
            .members
            .iter()
            .copied()
            .filter(|&k| ctx.world.can_move(k) && !ctx.world.mounted(k))
            .filter(|&k| ctx.world.squad[k].as_ref().is_some_and(|s| s.boarding.is_none()))
            .collect();
        if movers.is_empty() {
            continue;
        }
        let n = movers.len() as f64;
        let centre = [
            movers.iter().map(|&k| ctx.world.transform[k].pos[0]).sum::<f64>() / n,
            movers.iter().map(|&k| ctx.world.transform[k].pos[1]).sum::<f64>() / n,
        ];
        let spacing = ctx.doctrines[p.side.0 as usize].infantry.squad_spacing_m.get();
        let from = via.first().copied().unwrap_or(centre);
        let b = if crate::dist(from, *destination) > 1.0 { ai::drive::bearing(from, *destination) } else { p.heading };
        let slot = |i: usize| {
            let off = (i as f64 - (n - 1.0) / 2.0) * spacing;
            [destination[0] + libm::cos(b) * off, destination[1] - libm::sin(b) * off]
        };
        let idle =
            movers.iter().all(|&k| ctx.world.ai[k].as_ref().is_none_or(|a| a.order == crate::world::Order::Idle));
        if route.is_empty() {
            // New order: plan every squad's route.
            for (i, &k) in movers.iter().enumerate() {
                let pos = ctx.world.transform[k].pos;
                match foot_route(ctx.map, ctx.pathfinder, pos, via, slot(i)) {
                    Some(r) => ctx.world.set_route(k, r, p.speed_limit, Some(b), true, FOOT_ARRIVE_M),
                    None => ctx.events.push(Event::NoRoute { unit: ctx.world.id[k] }),
                }
            }
            *route = vec![centre, *destination];
            p.heading = b;
            continue;
        }
        if !idle {
            continue;
        }
        // Everyone stopped: there, or interrupted (a halt, a contact drill): re-plan the rest.
        let there = movers.iter().enumerate().all(|(i, &k)| crate::dist(ctx.world.transform[k].pos, slot(i)) < 15.0);
        if there {
            let at = *destination;
            p.order = PlatoonOrder::None;
            ctx.events.push(Event::PlatoonReport {
                side: p.side,
                platoon: p.id,
                report: crate::command::PlatoonReport::SetAt { pos: at },
            });
        } else if movers.iter().all(|&k| ctx.world.ai[k].as_ref().is_none_or(|a| a.held.is_none())) {
            route.clear();
        }
    }
}
