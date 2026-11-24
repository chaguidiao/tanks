//! React to obstacle and breaching (docs/plans/M8D-ENGINEERS.md §2–§3; DESIGN.md §14 #40).
//!
//! ATP 3-20.15 battle drill *react to obstacle*: a platoon whose route meets an obstacle it knows
//! it cannot cross halts short of it, reports, and by its ON OBSTACLE setting bypasses it
//! (ATP 3-90.4: when time and terrain allow), breaches it, or waits for orders. A breach follows
//! FM 3-34.2 (2000) SOSRA as far as the platoon itself goes: the breacher reduces a lane (a plough
//! or roller tank at its breaching speed, FM 20-32 (1998) ch. 10; or engineer squads by hand),
//! the rest overwatch from where they halted, then the platoon goes through the lane. Suppression
//! and obscuration are the commander's: other platoons, smoke, fire missions.

use ai::picture::ObstacleReaction;
use sim_data::VehicleRule;

use crate::command::PlatoonReport;
use crate::obstacles::{Lane, Passage};
use crate::platoon::{Platoon, PlatoonOrder};
use crate::platoon_ai::{Ctx, centroid};
use crate::world::Order;
use crate::{DT, Vec2, dist};

/// The breacher has reached the point of breach within this distance (m).
const AT_ENTRY_M: f64 = 15.0;
/// Engineers work at the lane head from within this distance (m).
const AT_HEAD_M: f64 = 20.0;
/// Hand breaching: the lane runs this far either side of the band (m).
const HAND_OVERRUN_M: f64 = 5.0;

/// The obstacle drill for one platoon (1 Hz), before its order step.
pub fn step(p: &mut Platoon, ctx: &mut Ctx) {
    if ctx.obstacles.list.is_empty() {
        return;
    }
    // Waiting for a lane: go on through it once one is open.
    if let Some(o) = p.wait_lane {
        match &p.held {
            Some(PlatoonOrder::Move { .. }) if p.order == PlatoonOrder::None => {
                if ctx.obstacles.list[o].lanes.iter().any(|l| !l.open) {
                    let held = p.held.take().expect("matched");
                    if let Some((dest, ahead)) = held.move_ahead() {
                        p.order = PlatoonOrder::new_move(dest, ahead);
                    }
                    p.wait_lane = None;
                }
            }
            _ => p.wait_lane = None, // another order came
        }
    }
    if matches!(p.order, PlatoonOrder::Breach { .. }) {
        breach_step(p, ctx);
        return;
    }
    if !matches!(p.order, PlatoonOrder::Move { .. }) || p.withdraw {
        return;
    }
    let Some((o, at)) = blocking(p, ctx) else { return };
    react(p, ctx, o, at);
}

/// Is `pt` closed by a known obstacle for this platoon? Vehicle platoons: closed to the leader's
/// type, or mined. Squads: closed on foot; engineers also stop at what closes the road to
/// vehicles (wire, abatis, a minefield) when ordered to breach, to open it for the tanks.
fn closed_at(p: &Platoon, ctx: &Ctx, leader: Option<usize>, engineers: bool, pt: Vec2) -> Option<usize> {
    let f = &*ctx.obstacles;
    let o = f.at(pt)?;
    let ob = &f.list[o];
    if !ob.known[p.side.0 as usize] || ob.lane_at(pt).is_some() {
        return None;
    }
    let model = ctx.obstacle_model;
    let rule = model.kinds[ob.kind].vehicle;
    let closed = match leader {
        Some(k) => {
            let v = &ctx.vehicles[ctx.world.vehicle[k]?];
            rule == VehicleRule::Mined || f.vehicle_passage(model, o, v, pt) == Passage::Blocked
        }
        None => {
            f.foot_passage(model, o, pt) == Passage::Blocked
                || (engineers && matches!(rule, VehicleRule::Mined | VehicleRule::NoGo | VehicleRule::Slow(_)))
        }
    };
    closed.then_some(o)
}

fn movers(p: &Platoon, ctx: &Ctx) -> Vec<usize> {
    p.members.iter().copied().filter(|&k| ctx.world.can_move(k) && !ctx.world.mounted(k)).collect()
}

fn engineers_of(p: &Platoon, ctx: &Ctx) -> Vec<usize> {
    movers(p, ctx)
        .into_iter()
        .filter(|&k| ctx.world.squad[k].as_ref().is_some_and(|s| !s.ineffective && ctx.squads[s.def].engineer))
        .collect()
}

/// The first point of the platoon's remaining route closed by a known obstacle, or a member
/// stopped at one.
fn blocking(p: &Platoon, ctx: &Ctx) -> Option<(usize, Vec2)> {
    let movers = movers(p, ctx);
    let leader = (!p.dismount).then(|| movers.first().copied()).flatten();
    let engineers = p.dismount && p.obstacle_reaction == ObstacleReaction::Breach && !engineers_of(p, ctx).is_empty();
    for &k in &movers {
        if let Some(&o) = ctx.obstacles.met.get(&k)
            && ctx.obstacles.list[o].known[p.side.0 as usize]
        {
            return Some((o, ctx.world.transform[k].pos));
        }
    }
    let mut paths: Vec<Vec<Vec2>> = Vec::new();
    match &p.order {
        PlatoonOrder::Move { route, progress, .. } if !p.dismount && !route.is_empty() => {
            paths.push(ai::formation::remaining(route, progress[0]));
        }
        _ => {
            for &k in &movers {
                if let Some(Order::Move { route, segment, .. }) = ctx.world.ai[k].as_ref().map(|a| &a.order) {
                    let mut r = vec![ctx.world.transform[k].pos];
                    r.extend(route.iter().skip(segment + 1).copied());
                    paths.push(r);
                }
            }
        }
    }
    let step = ctx.obstacles.cell_m * 0.5;
    for path in paths {
        for w in path.windows(2) {
            let n = (dist(w[0], w[1]) / step).ceil().max(1.0) as usize;
            for i in 0..=n {
                let t = i as f64 / n as f64;
                let pt = [w[0][0] + (w[1][0] - w[0][0]) * t, w[0][1] + (w[1][1] - w[0][1]) * t];
                if let Some(o) = closed_at(p, ctx, leader, engineers, pt) {
                    return Some((o, pt));
                }
            }
        }
    }
    None
}

fn report(p: &Platoon, ctx: &mut Ctx, r: PlatoonReport) {
    ctx.report(p, r);
}

/// Halt short of obstacle `o` with the move held, and wait for a lane or orders.
fn halt_for_lane(p: &mut Platoon, ctx: &mut Ctx, o: usize) {
    let resume = p.order.move_ahead();
    p.halt();
    if p.held.is_none()
        && let Some((d, ahead)) = resume
    {
        p.held = Some(PlatoonOrder::new_move(d, ahead));
    }
    p.wait_lane = Some(o);
    for &k in &p.members {
        ctx.world.halt(k);
    }
}

/// React to obstacle `o` met at `at` (ATP 3-20.15 battle drill react to obstacle).
fn react(p: &mut Platoon, ctx: &mut Ctx, o: usize, at: Vec2) {
    let reaction = p.obstacle_reaction;
    p.obstacle_tick = Some(ctx.tick);
    let name = ctx.obstacle_model.kinds[ctx.obstacles.list[o].kind].name.clone();
    report(p, ctx, PlatoonReport::Obstacle { kind: name, at, reaction });
    match reaction {
        ObstacleReaction::Halt => halt_for_lane(p, ctx, o),
        ObstacleReaction::Bypass => {
            if !bypass(p, ctx) {
                report(p, ctx, PlatoonReport::NoBypass);
                halt_for_lane(p, ctx, o);
            }
        }
        ObstacleReaction::Breach => {
            if !start_breach(p, ctx, o, at) {
                report(p, ctx, PlatoonReport::NoBreachAsset);
                halt_for_lane(p, ctx, o);
            }
        }
    }
}

/// Re-plan round what the side knows, if the detour is within the doctrine's factor of the
/// route that is left (ATP 3-90.4: bypass when time and terrain allow).
fn bypass(p: &mut Platoon, ctx: &mut Ctx) -> bool {
    let Some((dest, ahead)) = p.order.move_ahead() else { return false };
    let movers = movers(p, ctx);
    let Some(&lead) = movers.first() else { return false };
    let from = ctx.world.transform[lead].pos;
    let old: f64 = match &p.order {
        PlatoonOrder::Move { route, progress, .. } if !route.is_empty() && !p.dismount => {
            ai::formation::length(route) - progress[0]
        }
        _ => {
            let mut pts = vec![from];
            pts.extend(ahead.iter().copied());
            pts.push(dest);
            pts.windows(2).map(|w| dist(w[0], w[1])).sum()
        }
    };
    let new = if p.dismount {
        crate::infantry::foot_route(ctx.map, ctx.layers, p.side, ctx.pathfinder, from, &ahead, dest)
    } else {
        let avoid = crate::platoon_ai::avoids_built_up(p, ctx);
        ctx.route_via(lead, &ahead, dest, avoid).map(|r| r.0)
    };
    let Some(r) = new else { return false };
    let len: f64 = r.windows(2).map(|w| dist(w[0], w[1])).sum();
    if len > old.max(1.0) * ctx.obstacle_model.bypass_detour_factor.get() {
        return false;
    }
    p.order = PlatoonOrder::new_move(dest, ahead);
    true
}

/// Start a breach of obstacle `o` met at `at`, if the platoon has a plough or roller that can
/// reduce it, or engineer squads. The lane crosses the band square on at the nearest point of
/// its line, from the side the platoon is on.
fn start_breach(p: &mut Platoon, ctx: &mut Ctx, o: usize, at: Vec2) -> bool {
    let Some((dest, ahead)) = p.order.move_ahead() else { return false };
    let model = ctx.obstacle_model;
    let ob = &ctx.obstacles.list[o];
    let kind = &model.kinds[ob.kind];
    let movers = movers(p, ctx);
    let breacher = (!p.dismount && kind.kit_breach)
        .then(|| movers.iter().copied().find(|k| ctx.obstacles.kits.get(k).is_some_and(|s| !s.wrecked)))
        .flatten();
    let engineers = if p.dismount && (kind.hand_breach_s.is_some() || kind.vehicle == VehicleRule::Mined) {
        engineers_of(p, ctx)
    } else {
        Vec::new()
    };
    if breacher.is_none() && engineers.is_empty() {
        return false;
    }
    let q = ob.nearest(at);
    // The band's direction at q, and its normal pointing the way the platoon is going.
    let seg = ob
        .points
        .windows(2)
        .min_by(|a, b| crate::obstacles::seg_dist(q, a[0], a[1]).total_cmp(&crate::obstacles::seg_dist(q, b[0], b[1])))
        .expect("two points");
    let (tx, ty) = (seg[1][0] - seg[0][0], seg[1][1] - seg[0][1]);
    let l = (tx * tx + ty * ty).sqrt().max(1e-9);
    let mut n = [-ty / l, tx / l];
    let centre = centroid(ctx.world, &movers);
    let toward = [q[0] - centre[0], q[1] - centre[1]];
    if n[0] * toward[0] + n[1] * toward[1] < 0.0 {
        n = [-n[0], -n[1]];
    }
    let over = if breacher.is_some() { model.lane_overrun_m.get() } else { HAND_OVERRUN_M };
    let reach = ob.half + over;
    let (m, e) = (&ctx.map.manifest, 2.0 * ctx.map.manifest.cell_size_m);
    let clamp = |v: Vec2| [v[0].clamp(e, m.width_m() - e), v[1].clamp(e, m.height_m() - e)];
    let entry = clamp([q[0] - n[0] * reach, q[1] - n[1] * reach]);
    let exit = clamp([q[0] + n[0] * reach, q[1] + n[1] * reach]);
    // Everyone else halts in place and overwatches.
    for &k in &p.members {
        if Some(k) != breacher && !engineers.contains(&k) {
            ctx.world.halt(k);
        }
    }
    for &k in breacher.iter().chain(engineers.iter()) {
        route_to(ctx, k, entry, p.side);
    }
    report(p, ctx, PlatoonReport::Breaching { at: q });
    p.order = PlatoonOrder::Breach { obstacle: o, entry, exit, breacher, engineers, lane: None, resume: (dest, ahead) };
    true
}

/// Route `k` to `to`; from inside an obstacle band (where it was halted) there may be no planned
/// route out, so then straight back the way it came.
fn route_to(ctx: &mut Ctx, k: usize, to: Vec2, side: crate::SideId) {
    let from = ctx.world.transform[k].pos;
    if ctx.world.squad[k].is_some() {
        let r = crate::infantry::foot_route(ctx.map, ctx.layers, side, ctx.pathfinder, from, &[], to);
        ctx.world.set_route(k, r.unwrap_or_else(|| vec![from, to]), None, None, true, 3.0);
    } else {
        let r = ctx.route_for(k, to);
        ctx.world.set_route(k, r.unwrap_or_else(|| vec![from, to]), None, None, true, 5.0);
    }
}

fn idle(ctx: &Ctx, k: usize) -> bool {
    ctx.world.ai[k].as_ref().is_none_or(|a| matches!(a.order, Order::Idle | Order::Face { .. }))
}

fn breach_step(p: &mut Platoon, ctx: &mut Ctx) {
    let PlatoonOrder::Breach { obstacle: o, entry, exit, breacher, engineers, mut lane, resume } = p.order.clone()
    else {
        return;
    };
    let model = ctx.obstacle_model;
    let dt = crate::PLATOON_AI_EVERY as f64 * DT;
    let total = dist(entry, exit);
    let dir = [(exit[0] - entry[0]) / total.max(1e-9), (exit[1] - entry[1]) / total.max(1e-9)];
    let along = |pt: Vec2| ((pt[0] - entry[0]) * dir[0] + (pt[1] - entry[1]) * dir[1]).clamp(0.0, total);
    let fail = |p: &mut Platoon, ctx: &mut Ctx| {
        report(p, ctx, PlatoonReport::BreachFailed);
        p.order = PlatoonOrder::new_move(resume.0, resume.1.clone());
        halt_for_lane(p, ctx, o);
    };
    let done = |p: &mut Platoon, ctx: &mut Ctx, li: usize| {
        let l = &mut ctx.obstacles.list[o].lanes[li];
        l.b = exit;
        l.open = false;
        ctx.obstacles.touch(o);
        let q = ctx.obstacles.list[o].nearest([(entry[0] + exit[0]) / 2.0, (entry[1] + exit[1]) / 2.0]);
        report(p, ctx, PlatoonReport::LaneOpen { at: q });
        // Through the lane one at a time (FM 20-32 ch. 10: traffic in a 4.5 m lane is one way).
        p.formation = sim_data::Formation::Column;
        p.order = PlatoonOrder::new_move(resume.0, resume.1.clone());
    };
    match (breacher, engineers.is_empty()) {
        // Plough or roller.
        (Some(k), _) => {
            let kit = ctx.obstacles.kits.get(&k).cloned();
            if !ctx.world.can_move(k) || kit.as_ref().is_none_or(|s| s.wrecked) {
                return fail(p, ctx);
            }
            let kd = &model.kits[kit.expect("checked").kit];
            let pos = ctx.world.transform[k].pos;
            match lane {
                None => {
                    if dist(pos, entry) <= AT_ENTRY_M {
                        let missed = 1.0 - kd.clear_pressure.get().min(kd.clear_full_width.get());
                        let residual = missed * model.lane_follow_factor.get();
                        let half = model.lane_width_m.get() / 2.0;
                        let lanes = &mut ctx.obstacles.list[o].lanes;
                        lanes.push(Lane { a: entry, b: entry, half, residual, open: true });
                        lane = Some(lanes.len() - 1);
                        ctx.world.set_route(k, vec![pos, exit], Some(kd.breach_speed_mps.get()), None, false, 3.0);
                    } else if idle(ctx, k) {
                        route_to(ctx, k, entry, p.side);
                    }
                }
                Some(li) => {
                    let s = along(pos);
                    let l = &mut ctx.obstacles.list[o].lanes[li];
                    let head = [entry[0] + dir[0] * s, entry[1] + dir[1] * s];
                    if dist(head, entry) > dist(l.b, entry) {
                        l.b = head;
                    }
                    ctx.obstacles.touch(o);
                    if s >= total - 3.0 {
                        return done(p, ctx, li);
                    }
                    if idle(ctx, k) {
                        ctx.world.set_route(k, vec![pos, exit], Some(kd.breach_speed_mps.get()), None, false, 3.0);
                    }
                }
            }
        }
        // Engineers by hand.
        (None, false) => {
            let alive: Vec<usize> = engineers
                .iter()
                .copied()
                .filter(|&k| ctx.world.can_move(k) && ctx.world.squad[k].as_ref().is_some_and(|s| !s.ineffective))
                .collect();
            if alive.is_empty() {
                return fail(p, ctx);
            }
            let ob = &ctx.obstacles.list[o];
            let kind = &model.kinds[ob.kind];
            let rate = if kind.vehicle == VehicleRule::Mined {
                model.hand_breach_mine_m_per_min.get() / 60.0
            } else {
                total / kind.hand_breach_s.as_ref().map_or(f64::INFINITY, |s| s.get()).max(1.0)
            };
            match lane {
                None => {
                    if alive.iter().any(|&k| dist(ctx.world.transform[k].pos, entry) <= AT_ENTRY_M) {
                        let half = model.lane_width_m.get() / 2.0;
                        let lanes = &mut ctx.obstacles.list[o].lanes;
                        lanes.push(Lane { a: entry, b: entry, half, residual: 0.0, open: true });
                        lane = Some(lanes.len() - 1);
                    } else {
                        for &k in &alive {
                            if idle(ctx, k) {
                                route_to(ctx, k, entry, p.side);
                            }
                        }
                    }
                }
                Some(li) => {
                    let head = ctx.obstacles.list[o].lanes[li].b;
                    let stop = model.breach_suppression_stop.get();
                    let working = alive.iter().any(|&k| {
                        dist(ctx.world.transform[k].pos, head) <= AT_HEAD_M && ctx.world.suppression[k] < stop
                    });
                    let s = along(head) + if working { rate * dt } else { 0.0 };
                    let new_head = [entry[0] + dir[0] * s.min(total), entry[1] + dir[1] * s.min(total)];
                    ctx.obstacles.list[o].lanes[li].b = new_head;
                    ctx.obstacles.touch(o);
                    if s >= total {
                        return done(p, ctx, li);
                    }
                    // The squads follow the lane head, just behind it.
                    let behind = [new_head[0] - dir[0] * 2.0, new_head[1] - dir[1] * 2.0];
                    for &k in &alive {
                        if dist(ctx.world.transform[k].pos, behind) > 6.0 && idle(ctx, k) {
                            let from = ctx.world.transform[k].pos;
                            ctx.world.set_route(k, vec![from, behind], None, None, true, 3.0);
                        }
                    }
                }
            }
        }
        (None, true) => return fail(p, ctx),
    }
    if let PlatoonOrder::Breach { lane: l, .. } = &mut p.order {
        *l = lane;
    }
}
