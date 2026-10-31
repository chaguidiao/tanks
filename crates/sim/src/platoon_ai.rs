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

use ai::formation::{along, length, route_slot, slot_position};
use ai::picture::{AmbushReaction, Condition, ContactReaction, PerceivedWorld, PlatoonId, SideId};
use ai::position::{BpQuery, Candidate, assign, evaluate};
use sim_data::{DoctrinePack, Technique, VehicleDef};
use terrain::BakedMap;
use terrain::ground::Ground;
use terrain::path::Pathfinder;

use crate::command::{Addressee, Command, PlatoonReport, PositionKind, SmokeMode};
use crate::platoon::{Platoon, PlatoonOrder};
use crate::world::{Order, World};
use crate::{DT, Event, Vec2, dist};

/// Formation travel speed as a fraction of the slowest member's cross-country speed: room for
/// vehicles to keep station over rough ground (estimate).
pub const FORMATION_SPEED_FRACTION: f64 = 0.6;
/// Re-plan a member's route to its slot when the slot moved this far (m) or this often (s).
const REPLAN_DISTANCE: f64 = 25.0;
const REPLAN_EVERY_S: f64 = 4.0;
/// A contact counts as current if reported within this time (s).
const CONTACT_FRESH_S: f64 = 25.0;
/// Resume the mission after this long without being engaged (s).
const QUIET_RESUME_S: f64 = 45.0;
/// An enemy shot counts as current this long (s).
const SHOT_RECENT_S: f64 = 20.0;
/// After a drill ends quiet, sighting alone does not trigger it again for this long (s).
const DRILL_COOLDOWN_S: f64 = 150.0;
const SLOT_ARRIVE: f64 = 6.0;
const BP_ARRIVE: f64 = 1.5;
/// Search radius for hasty hull-down positions around the platoon (m).
const HASTY_RADIUS_M: f64 = 150.0;

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
    pub impacts: &'a [crate::fires::Impact],
    pub fires: &'a sim_data::SuppressionModel,
    /// Orders the platoon phase hands to the sim to carry out (smoke, fire support, drills); they
    /// apply right after the phase without radio delay.
    pub sim_cmds: &'a mut Vec<(SideId, Command)>,
    /// Supply points of every side and the tables resupply needs (M8c).
    pub supply: &'a mut [crate::logistics::SupplyPoint],
    pub loadouts: &'a [Vec<usize>],
    pub ammo: &'a [sim_data::AmmoDef],
    pub logistics: &'a sim_data::LogisticsModel,
    /// Speed rasters round built-up ground (crate::builtup).
    pub avoid: &'a crate::builtup::AvoidLayers,
    /// Carrier platoons whose plans called their squads back aboard this tick: the orders and
    /// plans of their dismount platoons end.
    pub recalled: Vec<PlatoonId>,
}

impl<'a> Ctx<'a> {
    pub(crate) fn report(&mut self, p: &Platoon, r: PlatoonReport) {
        self.events.push(Event::PlatoonReport { side: p.side, platoon: p.id, report: r });
    }

    /// The speed raster member `k`'s routes are planned on: its own, or the one round built-up
    /// ground (`avoid`).
    fn speed(&self, k: usize, avoid: bool) -> Option<&'a terrain::Grid<u8>> {
        let (map, layers): (&'a BakedMap, &'a crate::builtup::AvoidLayers) = (self.map, self.avoid);
        let id = &self.vehicles[self.world.vehicle[k]?].id;
        if avoid && let Some(g) = layers.get(&(self.world.side[k].0, id.clone())) {
            return Some(g);
        }
        Some(&map.mobility(id)?.speed)
    }

    fn route(&mut self, k: usize, to: Vec2, avoid: bool) -> Option<Vec<Vec2>> {
        let (cell, from) = (self.map.manifest.cell_size_m, self.world.transform[k].pos);
        let speed = self.speed(k, avoid)?;
        self.pathfinder.find(speed, cell, from, to)
    }

    /// Route through checkpoints; also returns the arc length of each checkpoint.
    fn route_via(&mut self, k: usize, via: &[Vec2], to: Vec2, avoid: bool) -> Option<(Vec<Vec2>, Vec<f64>)> {
        let (cell, from) = (self.map.manifest.cell_size_m, self.world.transform[k].pos);
        let speed = self.speed(k, avoid)?;
        self.pathfinder.find_via(speed, cell, from, via, to)
    }
}

/// Does the platoon route round built-up ground (plan M8F1-INFANTRY-AI §3; FM 3-06.11)? Tanks
/// without dismounted support do, unless the leg says BUILT-UP ENTER; carriers and platoons with
/// friendly squads on the ground close by do not.
fn avoids_built_up(p: &Platoon, ctx: &Ctx) -> bool {
    use ai::picture::BuiltUp;
    if p.built_up == BuiltUp::Enter || p.dismount {
        return false;
    }
    let w = &ctx.world;
    if p.members.iter().any(|&k| w.carrier[k].is_some()) {
        return false;
    }
    let alive = p.alive_members(w);
    if alive.is_empty() {
        return false;
    }
    let c = centroid(w, &alive);
    let r = ctx.doctrines[p.side.0 as usize].infantry.built_up.support_radius_m.get();
    let supported = (0..w.len()).any(|q| {
        w.side[q] == p.side
            && w.alive(q)
            && w.squad[q].as_ref().is_some_and(|s| s.mounted_in.is_none() && !s.ineffective)
            && dist(w.transform[q].pos, c) <= r
    });
    !supported
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
        if p.dismount {
            // Orders of dismount platoons: crate::infantry; plans run here like any platoon's.
            crate::plan::step(p, ctx);
            continue;
        }
        // Supply trucks fight nothing: no plans, no fire distribution, no contact drills.
        if p.members.iter().any(|&k| ctx.world.cargo[k].is_some()) {
            match p.order.clone() {
                PlatoonOrder::Move { .. } => {
                    let doctrine = ctx.doctrines[p.side.0 as usize].clone();
                    move_step(p, ctx, &doctrine)
                }
                PlatoonOrder::Deliver { .. } => deliver_step(p, ctx),
                _ => {}
            }
            continue;
        }
        let doctrine = ctx.doctrines[p.side.0 as usize].clone();
        if !matches!(p.order, PlatoonOrder::Move { .. }) {
            p.withdraw = false;
        }
        crate::plan::step(p, ctx);
        fire_distribution(p, ctx, &doctrine);
        indirect_fire_drill(p, &alive, ctx);
        ambush_drill(p, ctx, &doctrine);
        contact_drill(p, ctx, &doctrine);
        match p.order.clone() {
            PlatoonOrder::Move { .. } => move_step(p, ctx, &doctrine),
            PlatoonOrder::Occupy { .. } => occupy_step(p, ctx, &doctrine),
            PlatoonOrder::Contact => contact_step(p, ctx),
            PlatoonOrder::Resupply { .. } => resupply_step(p, ctx),
            PlatoonOrder::Tailgate { .. } | PlatoonOrder::Deliver { .. } | PlatoonOrder::None => {}
        }
    }
    tailgate_link(platoons, ctx);
    for pid in std::mem::take(&mut ctx.recalled) {
        for d in platoons.iter_mut().filter(|d| d.dismount && d.carrier_platoon == Some(pid)) {
            d.order = PlatoonOrder::None;
            d.plan = None;
            d.held = None;
        }
    }
}

/// ATP 3-20.15 App. F: react to indirect fire. A platoon under a burst on or near it clears the
/// impact area: a moving platoon drops its march-speed limit and keeps going; a static one stays
/// buttoned up (suppression) and holds its position.
fn indirect_fire_drill(p: &mut Platoon, alive: &[usize], ctx: &mut Ctx) {
    let m = ctx.fires;
    let (radius, window) = (m.react_radius_m.get(), m.react_window_s.get());
    if p.last_indirect.is_some_and(|t| (ctx.tick - t) as f64 * DT < m.react_cooldown_s.get()) {
        return;
    }
    let hit = ctx
        .impacts
        .iter()
        .filter(|i| !i.smoke && (ctx.tick - i.tick) as f64 * DT <= window)
        .find(|i| alive.iter().any(|&k| dist(ctx.world.transform[k].pos, i.pos) <= radius));
    let Some(i) = hit else { return };
    p.last_indirect = Some(ctx.tick);
    let clearing = matches!(p.order, PlatoonOrder::Move { .. });
    if clearing {
        p.speed_limit = None;
    }
    ctx.report(p, PlatoonReport::IndirectFire { at: i.pos, clearing });
}

fn fresh_contacts(pic: &PerceivedWorld) -> Vec<Vec2> {
    pic.live_contacts().filter(|c| pic.time_s - c.last_report_s < CONTACT_FRESH_S).map(|c| c.pos).collect()
}

/// Fresh contacts to hand out as targets: recognised supply trucks only when nothing else is in
/// view, as they do not shoot back (plan M8C-TAILGATE-TRUCKS §5).
fn fire_targets(pic: &PerceivedWorld) -> Vec<Vec2> {
    let fresh: Vec<_> = pic.live_contacts().filter(|c| pic.time_s - c.last_report_s < CONTACT_FRESH_S).collect();
    let truck = |c: &&ai::picture::Contact| c.kind == Some(ai::picture::ContactKind::Supply);
    let threats: Vec<Vec2> = fresh.iter().filter(|c| !truck(c)).map(|c| c.pos).collect();
    if threats.is_empty() { fresh.iter().map(|c| c.pos).collect() } else { threats }
}

pub(crate) fn centroid(world: &World, members: &[usize]) -> Vec2 {
    let n = members.len().max(1) as f64;
    [
        members.iter().map(|&k| world.transform[k].pos[0]).sum::<f64>() / n,
        members.iter().map(|&k| world.transform[k].pos[1]).sum::<f64>() / n,
    ]
}

fn fire_distribution(p: &Platoon, ctx: &mut Ctx, d: &DoctrinePack) {
    let pic = &ctx.pictures[p.side.0 as usize];
    // WEAPONS HOLD: no target is handed out (an ENGAGE order sets TIGHT first).
    for &k in &p.members {
        if ctx.world.weapons[k] == ai::picture::WeaponsStatus::Hold
            && let Some(g) = ctx.world.gun[k].as_mut()
        {
            g.assigned = None;
        }
    }
    let shooters: Vec<usize> = p
        .members
        .iter()
        .copied()
        .filter(|&k| ctx.world.alive(k) && ctx.world.condition[k] != Condition::FirepowerKill)
        .filter(|&k| ctx.world.weapons[k] != ai::picture::WeaponsStatus::Hold)
        .collect();
    let c = centroid(ctx.world, &shooters);
    let range = d.engagement_range_m.get();
    let targets: Vec<Vec2> = fire_targets(pic).into_iter().filter(|t| dist(*t, c) < range).collect();
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

/// React to ambush (near) (ATP 3-21.8 (2016) battle drill; ATP 3-20.15 App. F react to contact;
/// plan M8F1-INFANTRY-AI §2): rockets or missiles fired at the platoon from close range. REVERSE
/// OUT smokes and backs the vehicles out of the kill zone with their fronts to the shooter, then
/// fights from there under the contact drill; DRIVE THROUGH smokes and drives on at full speed
/// without halting; HALT leaves it to the contact drill. A platoon in a battle position, or
/// breaking contact, fights as it is.
fn ambush_drill(p: &mut Platoon, ctx: &mut Ctx, d: &DoctrinePack) {
    let Some((t, from)) = p.ambush else { return };
    if t <= p.ambush_handled {
        return;
    }
    p.ambush_handled = t;
    let alive = p.alive_members(ctx.world);
    if alive.is_empty() {
        return;
    }
    let at = centroid(ctx.world, &alive);
    let reaction = p.ambush_reaction;
    ctx.report(p, PlatoonReport::Ambush { at, from, reaction });
    let on_move = matches!(p.order, PlatoonOrder::Move { .. }) || p.order == PlatoonOrder::Contact;
    if !on_move || p.withdraw {
        return;
    }
    let r = &d.infantry.react_ambush;
    let screen = (p.side, Command::Smoke { to: Addressee::Platoon(p.id), mode: SmokeMode::Screen });
    match reaction {
        AmbushReaction::Halt => {}
        AmbushReaction::ReverseOut => {
            ctx.sim_cmds.push(screen);
            if let Some((destination, ahead)) = p.order.move_ahead() {
                p.resume = Some((destination, ahead, p.technique));
            }
            p.order = PlatoonOrder::Contact;
            p.quiet_since = ctx.tick;
            let back = r.reverse_m.get();
            let movers: Vec<usize> = alive.iter().copied().filter(|&k| ctx.world.can_move(k)).collect();
            for k in movers {
                let tr = ctx.world.transform[k];
                let to = [tr.pos[0] - libm::sin(tr.heading) * back, tr.pos[1] - libm::cos(tr.heading) * back];
                if let Some(ai) = ctx.world.ai[k].as_mut() {
                    ai.order = Order::Reverse { to };
                    ai.held = None;
                    ai.set = false;
                    ai.position_note = None;
                }
            }
        }
        AmbushReaction::DriveThrough => {
            ctx.sim_cmds.push(screen);
            p.speed_limit = None;
            p.drive_through_until = ctx.tick + (r.drive_through_s.get() / DT) as u64;
            if p.order == PlatoonOrder::Contact
                && let Some((dest, via, tech)) = p.resume.take()
            {
                p.technique = tech;
                p.order = PlatoonOrder::new_move(dest, via);
            }
        }
    }
}

fn contact_drill(p: &mut Platoon, ctx: &mut Ctx, d: &DoctrinePack) {
    if !d.halt_on_contact
        || ctx.tick < p.drive_through_until
        || p.withdraw
        || p.contact_reaction == ContactReaction::Continue
        || !matches!(p.order, PlatoonOrder::Move { .. })
    {
        return;
    }
    let pic = &ctx.pictures[p.side.0 as usize];
    if ctx.tick < p.drill_off_until && !pic.shots.iter().any(|s| !s.own && pic.time_s - s.time_s < SHOT_RECENT_S) {
        return;
    }
    let alive = p.alive_members(ctx.world);
    let c = centroid(ctx.world, &alive);
    let Some(threat) = fresh_contacts(pic)
        .into_iter()
        .filter(|t| dist(*t, c) < d.engagement_range_m.get())
        .min_by(|a, b| dist(*a, c).total_cmp(&dist(*b, c)))
    else {
        return;
    };
    if p.contact_reaction == ContactReaction::BreakContact {
        // Battle drill break contact (ATP 3-20.15 App. F): smoke and withdraw; abandons the plan.
        ctx.report(p, PlatoonReport::Contact { at: c, toward: threat });
        ctx.sim_cmds.push((p.side, Command::BreakContact { platoon: p.id }));
        return;
    }
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
    if p.contact_reaction == ContactReaction::HastyPosition {
        // Hasty positions (ATP 3-20.15 ch. 5): hull-down ground close by, facing the threat.
        assign_positions(p, ctx, d, c, HASTY_RADIUS_M, ai::drive::bearing(c, threat), PositionKind::HullDown);
    }
    ctx.report(p, PlatoonReport::Contact { at: c, toward: threat });
}

fn contact_step(p: &mut Platoon, ctx: &mut Ctx) {
    let pic = &ctx.pictures[p.side.0 as usize];
    // The drill holds while the platoon is engaged: its guns have a target, or an enemy shot has been
    // seen lately (ATP 3-20.15 App. F: react to contact, then continue the mission). An enemy that
    // is merely visible, out of range and silent does not freeze the platoon forever.
    let shot_at = pic.shots.iter().any(|s| !s.own && pic.time_s - s.time_s < SHOT_RECENT_S);
    if shot_at
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
                p.drill_off_until = ctx.tick + (DRILL_COOLDOWN_S / DT) as u64;
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
    let avoid = avoids_built_up(p, ctx);
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
        match ctx.route_via(leader, &via, destination, avoid) {
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
        // Slots are measured along the route, so trailing vehicles stay on it through bends and
        // village streets (TODO Z4: heading-rotated offsets put them off the street).
        route_slot(&route, s, off)
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
            if let Some(r) = ctx.route(k, sp, avoid) {
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

/// Tailgate resupply, truck side (plan M8C-TAILGATE-TRUCKS §3; FM 17-15 (1996), ATP 3-20.15
/// (2025): in the tailgate method the LOGPAC moves to the vehicles in their positions). The trucks
/// gather at a release point behind the platoon, then each takes in platoon order the next vehicle
/// it has something for that no other truck is serving, parks `tailgate_m` behind it and hands
/// over at the data rates; when no truck can give anything more they drive back. Fired on, they
/// withdraw to the release point (convoy react-to-fire drill: leave the kill zone).
fn deliver_step(p: &mut Platoon, ctx: &mut Ctx) {
    use crate::logistics::{Tables, can_give, transfer};
    use crate::platoon::DeliverPhase;
    let PlatoonOrder::Deliver { to, targets, release, home, mut phase, mut jobs } = p.order.clone() else { return };
    let m = ctx.logistics;
    let dt = crate::PLATOON_AI_EVERY as f64 * DT;
    let trucks: Vec<usize> = p.members.clone();
    let slot = |i: usize| -> Vec2 {
        // Trucks wait side by side at the release point.
        let off = (i as f64 - (trucks.len() as f64 - 1.0) / 2.0) * 15.0;
        [release[0] + off, release[1]]
    };
    let drive = |ctx: &mut Ctx, k: usize, st: &mut crate::logistics::ServiceState, goal: Vec2, arrive: f64| {
        let pos = ctx.world.transform[k].pos;
        let idle = ctx.world.ai[k].as_ref().is_some_and(|a| matches!(a.order, Order::Idle | Order::Face { .. }));
        let moved = st.goal[0].is_nan() || dist(st.goal, goal) > 2.0;
        if dist(pos, goal) > arrive && (moved || idle) {
            if let Some(r) = ctx.route(k, goal, false) {
                ctx.world.set_route(k, r, None, None, true, arrive);
            }
            st.goal = goal;
        }
        dist(pos, goal) <= arrive.max(8.0)
    };

    // React to fire: a burst close by, a shot at a truck (suppression) or a truck lost.
    if !matches!(phase, DeliverPhase::Return { .. }) {
        let window = ctx.fires.react_window_s.get();
        let burst = ctx.impacts.iter().any(|i| {
            !i.smoke
                && (ctx.tick - i.tick) as f64 * DT <= window
                && trucks.iter().any(|&k| dist(ctx.world.transform[k].pos, i.pos) <= m.withdraw_burst_m.get())
        });
        let fired_on = trucks.iter().any(|&k| ctx.world.suppression[k] > 0.0 || !ctx.world.can_move(k));
        if burst || fired_on {
            phase = DeliverPhase::Return { withdraw: true };
            for j in jobs.iter_mut() {
                j.serving = None;
                j.at_vehicle = false;
                j.st.goal = [f64::NAN, f64::NAN];
            }
            ctx.report(p, PlatoonReport::TrucksWithdrawing);
        }
    }

    match phase {
        DeliverPhase::Gather => {
            let mut all = true;
            for (i, &k) in trucks.iter().enumerate() {
                if ctx.world.can_move(k) {
                    all &= drive(ctx, k, &mut jobs[i].st, slot(i), 10.0);
                }
            }
            if all {
                phase = DeliverPhase::Serve;
            }
        }
        DeliverPhase::Serve => {
            let t = Tables { vehicles: ctx.vehicles, loadouts: ctx.loadouts, ammo: ctx.ammo, model: ctx.logistics };
            let mut busy = false;
            for (i, &k) in trucks.iter().enumerate() {
                if !ctx.world.can_move(k) {
                    jobs[i].serving = None;
                    continue;
                }
                // Drop a vehicle that is gone or has nothing more to take from this truck.
                if let Some(v) = jobs[i].serving {
                    let tk = targets[v];
                    let stock = ctx.world.cargo[k].as_ref().expect("a truck");
                    if !ctx.world.alive(tk) || !can_give(ctx.world, tk, stock, &t) {
                        jobs[i].finished.push(v);
                        jobs[i].serving = None;
                        jobs[i].at_vehicle = false;
                    }
                }
                if jobs[i].serving.is_none() {
                    let taken: Vec<usize> = jobs.iter().filter_map(|j| j.serving).collect();
                    let stock = ctx.world.cargo[k].as_ref().expect("a truck");
                    let fin = &jobs[i].finished;
                    jobs[i].serving = (0..targets.len()).find(|&v| {
                        !taken.contains(&v)
                            && !fin.contains(&v)
                            && ctx.world.alive(targets[v])
                            && can_give(ctx.world, targets[v], stock, &t)
                    });
                    jobs[i].st.credit = [0.0; 4];
                }
                let Some(v) = jobs[i].serving else {
                    drive(ctx, k, &mut jobs[i].st, slot(i), 10.0);
                    continue;
                };
                busy = true;
                let tk = targets[v];
                let (tp, h) = (ctx.world.transform[tk].pos, ctx.world.transform[tk].heading);
                let spot = [tp[0] - libm::sin(h) * m.tailgate_m.get(), tp[1] - libm::cos(h) * m.tailgate_m.get()];
                let there = drive(ctx, k, &mut jobs[i].st, spot, 4.0);
                let still = |w: &World, q: usize| w.mobility[q].as_ref().is_some_and(|mm| mm.speed.abs() < 0.5);
                jobs[i].at_vehicle = there && still(ctx.world, k) && still(ctx.world, tk);
                if jobs[i].at_vehicle {
                    ctx.world.halt(k);
                    let mut stock = ctx.world.cargo[k].take().expect("a truck");
                    let mut st = jobs[i].st.clone();
                    let done = transfer(ctx.world, tk, &mut stock, &mut st, &t, dt, ctx.tick);
                    jobs[i].st.credit = st.credit;
                    ctx.world.cargo[k] = Some(stock);
                    if done {
                        jobs[i].finished.push(v);
                        jobs[i].serving = None;
                        jobs[i].at_vehicle = false;
                    }
                }
            }
            if !busy {
                phase = DeliverPhase::Return { withdraw: false };
                for j in jobs.iter_mut() {
                    j.st.goal = [f64::NAN, f64::NAN];
                }
                let left: Vec<&crate::logistics::Stock> = trucks
                    .iter()
                    .filter(|&&k| ctx.world.alive(k))
                    .filter_map(|&k| ctx.world.cargo[k].as_ref())
                    .collect();
                let fuel_l = left.iter().map(|s| s.fuel_l).sum();
                let rounds = left.iter().map(|s| s.rounds.iter().sum::<u32>()).sum();
                ctx.report(p, PlatoonReport::TrucksReturning { fuel_l, rounds });
            }
        }
        DeliverPhase::Return { withdraw } => {
            let mut all = true;
            for (i, &k) in trucks.iter().enumerate() {
                jobs[i].serving = None;
                jobs[i].at_vehicle = false;
                if ctx.world.can_move(k) {
                    let goal = if withdraw { slot(i) } else { home[i] };
                    all &= drive(ctx, k, &mut jobs[i].st, goal, 10.0);
                }
            }
            if all {
                p.order = PlatoonOrder::None;
                return;
            }
        }
    }
    p.order = PlatoonOrder::Deliver { to, targets, release, home, phase, jobs };
}

/// Tailgate resupply, platoon side: the served platoon's per-vehicle stage follows the trucks;
/// when the trucks leave (done or withdrawing) it reports and its order ends. Trucks whose
/// platoon was given another order go home.
fn tailgate_link(platoons: &mut [Platoon], ctx: &mut Ctx) {
    use crate::logistics::{Tables, can_give, needs_worth};
    use crate::platoon::DeliverPhase;
    use ai::picture::ServiceStage;
    let t = Tables { vehicles: ctx.vehicles, loadouts: ctx.loadouts, ammo: ctx.ammo, model: ctx.logistics };
    for ti in 0..platoons.len() {
        let PlatoonOrder::Deliver { to, targets, phase, jobs, .. } = platoons[ti].order.clone() else { continue };
        let trucks_id = platoons[ti].id;
        let Some(si) = platoons.iter().position(|q| q.id == to) else { continue };
        let served = matches!(&platoons[si].order, PlatoonOrder::Tailgate { trucks, .. } if *trucks == trucks_id);
        if !served {
            if !matches!(phase, DeliverPhase::Return { .. })
                && let PlatoonOrder::Deliver { phase, jobs, .. } = &mut platoons[ti].order
            {
                *phase = DeliverPhase::Return { withdraw: false };
                for j in jobs.iter_mut() {
                    j.serving = None;
                    j.st.goal = [f64::NAN, f64::NAN];
                }
            }
            continue;
        }
        let live: Vec<(usize, usize)> =
            platoons[ti].members.iter().copied().enumerate().filter(|&(_, k)| ctx.world.can_move(k)).collect();
        let w = &*ctx.world;
        let PlatoonOrder::Tailgate { members, .. } = &mut platoons[si].order else { continue };
        for (v, st) in members.iter_mut().enumerate() {
            let k = targets[v];
            let any_truck = live.iter().any(|&(i, q)| {
                !jobs[i].finished.contains(&v) && w.cargo[q].as_ref().is_some_and(|s| can_give(w, k, s, &t))
            });
            st.done = !w.alive(k) || !any_truck;
            // Short: it still lacks something no live truck carries any more.
            let stocked = live.iter().any(|&(_, q)| w.cargo[q].as_ref().is_some_and(|s| can_give(w, k, s, &t)));
            st.short = w.alive(k) && st.done && needs_worth(w, k, &t) && !stocked;
            st.stage = if st.done {
                ServiceStage::Done
            } else if jobs.iter().any(|j| j.serving == Some(v) && j.at_vehicle) {
                ServiceStage::Served
            } else {
                ServiceStage::Waiting
            };
        }
        if let DeliverPhase::Return { withdraw } = phase {
            // Withdrawn trucks leave vehicles unserved: partial.
            let complete = !members.iter().any(|m| m.short || (withdraw && !m.done));
            platoons[si].order = PlatoonOrder::None;
            ctx.report(&platoons[si], PlatoonReport::Resupplied { complete });
        }
    }
}

/// Service-station resupply (FM 17-15 (1996), ATP 3-20.15 (2025) sustainment): the first
/// `stations` members that still need anything drive into the supply point and are served there;
/// the rest wait in a holding line on the approach side. When every member is done the platoon
/// reports and holds. Members that cannot move are skipped.
fn resupply_step(p: &mut Platoon, ctx: &mut Ctx) {
    let avoid = avoids_built_up(p, ctx);
    use crate::logistics::{Tables, transfer};
    use ai::picture::ServiceStage;
    let PlatoonOrder::Resupply { point, approach, mut members } = p.order.clone() else { return };
    let Some(sp_pos) = ctx.supply.get(point).map(|s| s.pos) else {
        p.order = PlatoonOrder::None;
        return;
    };
    let radius = ctx.logistics.point_radius_m.get();
    let stations = ctx.logistics.stations.get().max(1.0) as usize;
    let dt = crate::PLATOON_AI_EVERY as f64 * DT;
    let (sa, ca) = (libm::sin(approach), libm::cos(approach));
    let holding = |j: usize| -> Vec2 {
        let d = radius + 40.0 + 25.0 * j as f64;
        [sp_pos[0] + sa * d, sp_pos[1] + ca * d]
    };
    let mut serving = 0;
    for (i, &k) in p.members.iter().enumerate() {
        let st = &mut members[i];
        if !ctx.world.can_move(k) {
            st.done = true;
            st.stage = ServiceStage::Done;
            continue;
        }
        let active = !st.done && serving < stations;
        if active {
            serving += 1;
        }
        let pos = ctx.world.transform[k].pos;
        let at_point = dist(pos, sp_pos) <= radius;
        let slow = ctx.world.mobility[k].as_ref().is_some_and(|m| m.speed.abs() < 0.5);
        if active && at_point && slow {
            ctx.world.halt(k);
            let t = Tables { vehicles: ctx.vehicles, loadouts: ctx.loadouts, ammo: ctx.ammo, model: ctx.logistics };
            st.done = transfer(ctx.world, k, &mut ctx.supply[point].stock, st, &t, dt, ctx.tick);
            st.short = st.done && crate::logistics::needs_anything(ctx.world, k, &t);
            st.stage = if st.done { ServiceStage::Done } else { ServiceStage::Served };
            continue;
        }
        st.stage = match (st.done, active) {
            (true, _) => ServiceStage::Done,
            (false, true) => ServiceStage::ToPoint,
            (false, false) => ServiceStage::Waiting,
        };
        let (goal, arrive) = if active { (sp_pos, radius * 0.5) } else { (holding(i), SLOT_ARRIVE) };
        let idle = ctx.world.ai[k].as_ref().is_some_and(|a| matches!(a.order, Order::Idle | Order::Face { .. }));
        let moved = st.goal[0].is_nan() || dist(st.goal, goal) > 1.0;
        if dist(pos, goal) > arrive && (moved || idle) {
            if let Some(r) = ctx.route(k, goal, avoid) {
                ctx.world.set_route(k, r, p.speed_limit, None, true, arrive);
            }
            st.goal = goal;
        }
    }
    if members.iter().all(|m| m.done) {
        let complete = !members.iter().any(|m| m.short);
        p.order = PlatoonOrder::None;
        ctx.report(p, PlatoonReport::Resupplied { complete });
    } else {
        p.order = PlatoonOrder::Resupply { point, approach, members };
    }
}

/// Pick firing positions for the movers in a circle and send them there (shared by the battle
/// position order and the hasty-position contact reaction).
fn assign_positions(
    p: &mut Platoon,
    ctx: &mut Ctx,
    d: &DoctrinePack,
    centre: Vec2,
    radius: f64,
    facing: f64,
    kind: PositionKind,
) {
    let avoid = avoids_built_up(p, ctx);
    let movers: Vec<usize> = p.members.iter().copied().filter(|&k| ctx.world.can_move(k)).collect();
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
        if let Some(r) = ctx.route(k, c.pos, avoid) {
            ctx.world.set_route(k, r, None, Some(facing), true, BP_ARRIVE);
        }
        p.bp.push((k, c));
    }
}

fn occupy_step(p: &mut Platoon, ctx: &mut Ctx, d: &DoctrinePack) {
    let PlatoonOrder::Occupy { centre, radius, facing, kind, assigned, reported } = p.order.clone() else { return };
    if !assigned {
        assign_positions(p, ctx, d, centre, radius, facing, kind);
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
