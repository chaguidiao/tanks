//! Checkpoint plans (DESIGN.md §8.3, §14 #22–#24): a runner in the 1 Hz platoon phase turns the
//! current plan step into the platoon's existing orders. No new movement code: legs are
//! `PlatoonOrder::Move` with pass-through checkpoints as `via` (decision #16).
//!
//! - Checkpoints and routes: FM 1-02.2, ADP 3-90.
//! - Execution on conditions ("on order" / by code word): FM 6-0 execution matrix.
//! - Leg settings (speed, formation, technique, weapons): per-leg tactics as in ATP 3-20.15 ch. 3.

use ai::picture::{ConditionSet, LegSettings, Plan, PlanView, PlatoonId, SpeedSetting, StepAction, TriggerCondition};
use terrain::hash::StableHasher;

use crate::command::{Addressee, Command, PlatoonReport, SmokeMode};
use crate::control;
use crate::platoon::{Platoon, PlatoonOrder};
use crate::platoon_ai::{Ctx, centroid};
use crate::world::Order;
use crate::{DT, dist};

/// A move counts as having reached its checkpoint when a member is this close (m).
const ARRIVE_TOLERANCE_M: f64 = 60.0;
/// Already at the first checkpoint of a leg (m): no move is issued.
const AT_CHECKPOINT_M: f64 = 25.0;
/// A plan holding for a troop drill (dismount, mount, occupy a building) goes on after this
/// long even if the drill has not ended, e.g. a squad that cannot reach its carrier (s).
const DRILL_TIMEOUT_S: f64 = 300.0;
/// A checkpoint's OCCUPY BUILDING takes a building whose centre is this close (m).
pub const OCCUPY_BUILDING_RADIUS_M: f64 = 60.0;

#[derive(Debug, Clone, PartialEq)]
pub enum Phase {
    /// The leg to the cursor step is issued on the next tick.
    Start,
    /// Moving; `stop` = index of the step this move ends at.
    Leg { stop: usize },
    /// Arrived at the cursor step and setting into the battle position its actions ordered;
    /// `since` = arrival tick.
    Setting { since: u64 },
    /// Holding at the cursor step. `signs` = reference sides for line-crossing conditions.
    Waiting { since: u64, signs: Vec<f64> },
    /// A troop drill its actions started (dismount, mount, occupy a building) is under way;
    /// `released` = started by the release actions (then the step is done when it ends).
    Drill { since: u64, released: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlanState {
    pub plan: Plan,
    /// Index of the step being executed.
    pub cursor: usize,
    pub phase: Phase,
    pub passed: Vec<u16>,
    /// Halted by the player: nothing advances until RESUME.
    pub paused: bool,
    /// Branch condition references of the cursor step: (step id, tick the step began, signs).
    branch_ref: Option<(u16, u64, Vec<f64>)>,
}

impl PlanState {
    pub fn new(plan: Plan, cursor: usize) -> Self {
        Self { plan, cursor, phase: Phase::Start, passed: Vec::new(), paused: false, branch_ref: None }
    }

    pub fn cursor_id(&self) -> u16 {
        self.plan.steps[self.cursor].id
    }

    /// Replace the plan by an edited one while it runs, keeping progress when `start` is the
    /// step being executed (DESIGN.md §14 #22).
    pub fn edited(self, plan: Plan, start: usize) -> Self {
        let same_step = plan.steps[start].id == self.cursor_id();
        let phase = match self.phase {
            Phase::Waiting { since, signs } if same_step && plan.steps[start].wait.is_some() => {
                Phase::Waiting { since, signs }
            }
            Phase::Setting { since } if same_step => Phase::Setting { since },
            _ => Phase::Start,
        };
        Self { plan, cursor: start, phase, passed: self.passed, paused: self.paused, branch_ref: None }
    }

    /// A posture order given while the plan runs (DESIGN.md §14 #25): write the new setting into
    /// the leg in progress, i.e. the steps from the cursor to the next stop, so a pass-through
    /// checkpoint does not undo it. While acting or waiting at a checkpoint there is no leg in
    /// progress and the next leg keeps its planned setting.
    pub fn rewrite_leg(&mut self, set: impl Fn(&mut LegSettings)) {
        let stop = match self.phase {
            Phase::Leg { stop } => stop,
            Phase::Start => {
                let steps = &self.plan.steps;
                let last = steps.len() - 1;
                (self.cursor..=last).find(|&i| i == last || steps[i].is_stop()).unwrap_or(last)
            }
            Phase::Setting { .. } | Phase::Waiting { .. } | Phase::Drill { .. } => return,
        };
        for s in &mut self.plan.steps[self.cursor..=stop.max(self.cursor)] {
            set(&mut s.leg);
        }
    }

    pub fn view(&self) -> PlanView {
        let step = &self.plan.steps[self.cursor];
        PlanView {
            plan: self.plan.clone(),
            cursor_id: step.id,
            waiting: match self.phase {
                Phase::Waiting { .. } => step.wait.clone(),
                _ => None,
            },
            passed: self.passed.clone(),
        }
    }

    pub fn hash_into(&self, id: PlatoonId, h: &mut StableHasher) {
        h.u32(id.0);
        h.u64(self.cursor as u64);
        h.u8(self.paused as u8);
        match &self.phase {
            Phase::Start => h.u8(0),
            Phase::Leg { stop } => {
                h.u8(1);
                h.u64(*stop as u64);
            }
            Phase::Waiting { since, .. } => {
                h.u8(2);
                h.u64(*since);
            }
            Phase::Setting { since } => {
                h.u8(3);
                h.u64(*since);
            }
            Phase::Drill { since, released } => {
                h.u8(4);
                h.u64(*since);
                h.u8(*released as u8);
            }
        }
        h.u64(self.passed.len() as u64);
        for s in &self.plan.steps {
            let l = &s.leg;
            h.u32(s.id as u32);
            h.u8(match l.speed {
                None => 0,
                Some(SpeedSetting::Max) => 1,
                Some(SpeedSetting::Limit(_)) => 2,
            });
            if let Some(SpeedSetting::Limit(v)) = l.speed {
                h.f64(v);
            }
            h.u8(l.formation.map_or(255, |f| f as u8));
            h.u8(l.technique.map_or(255, |t| t as u8));
            h.u8(l.weapons.map_or(255, |w| w as u8));
            h.u8(l.on_contact.map_or(255, |r| r as u8));
        }
    }
}

/// Run one platoon's plan (1 Hz).
pub fn step(p: &mut Platoon, ctx: &mut Ctx) {
    let Some(mut st) = p.plan.take() else { return };
    if run(&mut st, p, ctx) {
        p.plan = Some(st);
    }
}

/// Returns false when the plan is over (finished or abandoned).
fn run(st: &mut PlanState, p: &mut Platoon, ctx: &mut Ctx) -> bool {
    if st.paused {
        return true;
    }
    if branch(st, p, ctx) {
        return true;
    }
    // A bounded chain of transitions per tick (arrive -> act -> next leg).
    for _ in 0..8 {
        match st.phase.clone() {
            Phase::Setting { since } => match p.order {
                PlatoonOrder::Occupy { reported: true, .. } => {
                    if !finish_arrival(st, p, ctx, since) {
                        return false;
                    }
                }
                PlatoonOrder::Occupy { .. } => return true,
                _ => return false,
            },
            Phase::Drill { since, released } => {
                let timed_out = (ctx.tick - since) as f64 * DT >= DRILL_TIMEOUT_S;
                if since == ctx.tick || (troops_busy(p, ctx) && !timed_out) {
                    return true;
                }
                let done = if released { advance(st, p, ctx) } else { finish_arrival(st, p, ctx, since) };
                if !done {
                    return false;
                }
            }
            Phase::Start => {
                if !issue_leg(st, p, ctx) {
                    return true;
                }
            }
            Phase::Leg { stop } => match &p.order {
                PlatoonOrder::Move { .. } | PlatoonOrder::Contact => {
                    track_progress(st, p, ctx, stop);
                    return true;
                }
                PlatoonOrder::None => {
                    if !arrived(p, ctx, st.plan.steps[stop].at) {
                        return false; // route failed or the move was cancelled
                    }
                    if !arrive(st, p, ctx, stop) {
                        return false;
                    }
                }
                PlatoonOrder::Occupy { .. } | PlatoonOrder::Resupply { .. } => return false,
            },
            Phase::Waiting { since, signs } => {
                let step = &st.plan.steps[st.cursor];
                let Some(set) = &step.wait else {
                    st.phase = Phase::Start;
                    continue;
                };
                let pic = &ctx.pictures[p.side.0 as usize];
                if !set_met(pic, set, &signs, since, ctx.tick) {
                    return true;
                }
                let (actions, at) = (step.on_release.clone(), step.at);
                let n = st.cursor as u16 + 1;
                if run_actions(&actions, n, at, p, ctx) {
                    st.phase = Phase::Drill { since: ctx.tick, released: true };
                    return true;
                }
                if !advance(st, p, ctx) {
                    return false;
                }
            }
        }
    }
    true
}

/// The move ended at checkpoint `at` (not a failed route): a vehicle that can still move stands in
/// its formation slot there. The leader's slot is on the checkpoint, but in a line or after losses
/// the nearest survivor can be a slot or two away from it.
fn arrived(p: &Platoon, ctx: &Ctx, at: crate::Vec2) -> bool {
    let d = &ctx.doctrines[p.side.0 as usize];
    if p.dismount {
        // Squads end on line across the move, `squad_spacing_m` apart (crate::infantry).
        let on_foot: Vec<usize> =
            p.members.iter().copied().filter(|&k| ctx.world.can_move(k) && !ctx.world.mounted(k)).collect();
        let half = on_foot.len().saturating_sub(1) as f64 / 2.0 * d.infantry.squad_spacing_m.get();
        return on_foot.iter().any(|&k| dist(ctx.world.transform[k].pos, at) <= ARRIVE_TOLERANCE_M + half);
    }
    let spacing = d.spacing_m.get();
    let slots = &d.formation(p.formation).slots;
    p.members.iter().enumerate().filter(|&(_, &k)| ctx.world.can_move(k)).any(|(i, &k)| {
        let off = slots.get(i).map_or(i as f64, |s| libm::hypot(s.0, s.1)) * spacing;
        dist(ctx.world.transform[k].pos, at) <= ARRIVE_TOLERANCE_M + off
    })
}

/// Issue the move for the leg(s) from the cursor to the next stop. False = nothing to do yet.
fn issue_leg(st: &mut PlanState, p: &mut Platoon, ctx: &mut Ctx) -> bool {
    let steps = &st.plan.steps;
    let last = steps.len() - 1;
    let stop = (st.cursor..=last).find(|&i| i == last || steps[i].is_stop()).unwrap_or(last);
    let via: Vec<_> = steps[st.cursor..stop].iter().map(|s| s.at).collect();
    let dest = steps[stop].at;
    apply_leg(st, p, ctx, st.cursor);
    let alive = p.alive_members(ctx.world);
    if alive.is_empty() {
        return false;
    }
    st.phase = Phase::Leg { stop };
    if via.is_empty() && dist(centroid(ctx.world, &alive), dest) <= AT_CHECKPOINT_M {
        p.order = PlatoonOrder::None;
        return true;
    }
    if p.order == PlatoonOrder::Contact {
        // The contact drill is under way: the mission resumes towards the new geometry.
        p.resume = Some((dest, via, p.technique));
        return true;
    }
    p.order = PlatoonOrder::new_move(dest, via);
    p.held = None;
    p.resume = None;
    p.withdraw = false;
    p.replanned = 0;
    p.contact_reported = false;
    true
}

/// Settings of the leg that ends at step `idx` take effect when the platoon heads for it.
fn apply_leg(st: &PlanState, p: &mut Platoon, ctx: &mut Ctx, idx: usize) {
    let leg = &st.plan.steps[idx].leg;
    match leg.speed {
        Some(SpeedSetting::Max) => p.speed_limit = None,
        Some(SpeedSetting::Limit(v)) => p.speed_limit = Some(v),
        None => {}
    }
    if let Some(f) = leg.formation {
        p.formation = f;
        p.replanned = 0;
    }
    if let Some(t) = leg.technique {
        p.technique = t;
    }
    if let Some(w) = leg.weapons {
        for &k in &p.members {
            ctx.world.weapons[k] = w;
        }
    }
    if let Some(r) = leg.on_contact {
        p.contact_reaction = r;
    }
}

/// Pass-through checkpoints behind the lead section are done: advance the cursor and apply the
/// settings of the leg that now begins.
fn track_progress(st: &mut PlanState, p: &mut Platoon, ctx: &mut Ctx, stop: usize) {
    let ahead = match &p.order {
        PlatoonOrder::Move { .. } => p.order.move_ahead().map_or(0, |(_, v)| v.len()),
        PlatoonOrder::Contact => p.resume.as_ref().map_or(0, |(_, v, _)| v.len()),
        _ => return,
    };
    let next = stop.saturating_sub(ahead);
    while st.cursor < next {
        let id = st.cursor_id();
        st.passed.push(id);
        st.cursor += 1;
        apply_leg(st, p, ctx, st.cursor);
    }
}

fn arrive(st: &mut PlanState, p: &mut Platoon, ctx: &mut Ctx, stop: usize) -> bool {
    while st.cursor < stop {
        let id = st.cursor_id();
        st.passed.push(id);
        st.cursor += 1;
    }
    let n = st.cursor as u16 + 1;
    let step = st.plan.steps[st.cursor].clone();
    if step.wait.is_some() {
        ctx.report(p, PlatoonReport::AtCheckpoint { n, waiting: true });
    }
    if run_actions(&step.actions, n, step.at, p, ctx) {
        st.phase = Phase::Drill { since: ctx.tick, released: false };
        return true;
    }
    if matches!(p.order, PlatoonOrder::Occupy { reported: false, .. }) {
        st.phase = Phase::Setting { since: ctx.tick };
        return true;
    }
    finish_arrival(st, p, ctx, ctx.tick)
}

/// Actions done (battle position set): hold for the wait condition, or go on.
fn finish_arrival(st: &mut PlanState, p: &mut Platoon, ctx: &mut Ctx, since: u64) -> bool {
    if let Some(set) = &st.plan.steps[st.cursor].wait {
        let pic = &ctx.pictures[p.side.0 as usize];
        let signs = set.conditions.iter().map(|c| control::reference_sign(pic, c)).collect();
        st.phase = Phase::Waiting { since, signs };
        true
    } else {
        advance(st, p, ctx)
    }
}

/// Contingency: when the cursor step's branch condition holds, go to the branch step instead
/// (FM 6-0 branch plan; the platoon moves from where it is). True = jumped.
fn branch(st: &mut PlanState, p: &mut Platoon, ctx: &mut Ctx) -> bool {
    let step = &st.plan.steps[st.cursor];
    let Some(b) = &step.branch else { return false };
    let Some(idx) = st.plan.index_of(b.goto) else { return false };
    if idx == st.cursor {
        return false;
    }
    let pic = &ctx.pictures[p.side.0 as usize];
    if st.branch_ref.as_ref().is_none_or(|(id, ..)| *id != step.id) {
        let signs = b.when.conditions.iter().map(|c| control::reference_sign(pic, c)).collect();
        st.branch_ref = Some((step.id, ctx.tick, signs));
    }
    let (_, since, signs) = st.branch_ref.clone().expect("set above");
    if !set_met(pic, &b.when, &signs, since, ctx.tick) {
        return false;
    }
    ctx.report(p, PlatoonReport::PlanBranch { to: idx as u16 + 1 });
    st.cursor = idx;
    st.phase = Phase::Start;
    st.branch_ref = None;
    true
}

/// Step done: go on to the next one, or finish the plan.
fn advance(st: &mut PlanState, p: &mut Platoon, ctx: &mut Ctx) -> bool {
    let id = st.cursor_id();
    st.passed.push(id);
    if st.cursor + 1 >= st.plan.steps.len() {
        p.finished = st.passed.clone();
        ctx.report(p, PlatoonReport::PlanComplete);
        return false;
    }
    st.cursor += 1;
    st.phase = Phase::Start;
    true
}

/// Carry out a step's actions. True = a troop drill started and the plan holds for it.
fn run_actions(actions: &[StepAction], n: u16, at: crate::Vec2, p: &mut Platoon, ctx: &mut Ctx) -> bool {
    let to = Addressee::Platoon(p.id);
    let mut drill = false;
    for a in actions {
        // Squads have no turret, smoke grenades or hull-down positions.
        let vehicle_only = matches!(
            a,
            StepAction::Face { .. }
                | StepAction::OccupyBp { .. }
                | StepAction::SmokeScreen
                | StepAction::EngineSmoke(_)
                | StepAction::FireSmoke { .. }
        );
        if p.dismount && vehicle_only {
            continue;
        }
        match a {
            StepAction::Face { bearing } => {
                for k in p.alive_members(ctx.world) {
                    if let Some(ai) = ctx.world.ai[k].as_mut() {
                        ai.order = Order::Face { heading: *bearing };
                    }
                }
            }
            StepAction::Report => ctx.report(p, PlatoonReport::AtCheckpoint { n, waiting: false }),
            // Battle position at the checkpoint (ATP 3-20.15 ch. 5); the plan holds until it is set.
            StepAction::OccupyBp { radius_m, facing, kind } => {
                p.order = PlatoonOrder::Occupy {
                    centre: at,
                    radius: *radius_m,
                    facing: *facing,
                    kind: *kind,
                    assigned: false,
                    reported: false,
                };
                p.held = None;
                p.withdraw = false;
            }
            StepAction::SmokeScreen => ctx.sim_cmds.push((p.side, Command::Smoke { to, mode: SmokeMode::Screen })),
            StepAction::EngineSmoke(on) => {
                let mode = if *on { SmokeMode::EngineOn } else { SmokeMode::EngineOff };
                ctx.sim_cmds.push((p.side, Command::Smoke { to, mode }));
            }
            StepAction::FireSmoke { at, rounds } => {
                ctx.sim_cmds.push((p.side, Command::FireSmoke { to, at: *at, rounds: *rounds }));
            }
            StepAction::CallForFire { battery, at, kind, volleys } => {
                ctx.sim_cmds.push((
                    p.side,
                    Command::CallForFire { battery: *battery, at: *at, kind: *kind, volleys: *volleys },
                ));
            }
            // ATP 3-21.71 (2010) ch. 3: dismount at the dismount point, establish local security.
            StepAction::Dismount if !p.dismount => {
                let carriers = p.alive_members(ctx.world);
                drill |= crate::infantry::order_dismount(ctx.world, ctx.vehicles, &carriers);
            }
            // ATP 3-21.71 ch. 3: mount at the pick-up point; the squads' own orders end.
            StepAction::Mount if !p.dismount => {
                let squads: Vec<usize> = (0..ctx.world.len())
                    .filter(|&q| {
                        ctx.world.squad[q].as_ref().and_then(|s| s.carrier).is_some_and(|c| p.members.contains(&c))
                    })
                    .collect();
                if !crate::infantry::order_mount(ctx.world, ctx.vehicles, &squads).is_empty() {
                    ctx.recalled.push(p.id);
                    drill = true;
                }
            }
            StepAction::OccupyBuilding if p.dismount => {
                if let Some(b) = ctx.map.enterable_building_near(at, OCCUPY_BUILDING_RADIUS_M) {
                    ctx.sim_cmds.push((p.side, Command::OccupyBuilding { to, building: b.id }));
                    drill = true;
                }
            }
            StepAction::Dismount | StepAction::Mount | StepAction::OccupyBuilding => {}
        }
    }
    drill
}

/// A troop drill of the platoon is under way: a carrier still in its ramp drill, a squad of its
/// carriers walking back to board, or (dismount platoon) a squad still on its way.
fn troops_busy(p: &Platoon, ctx: &Ctx) -> bool {
    let w = &ctx.world;
    let ramp =
        p.members.iter().any(|&c| w.alive(c) && w.carrier[c].as_ref().is_some_and(|cs| cs.dismounting.is_some()));
    let boarding = (0..w.len()).any(|q| {
        w.alive(q)
            && w.squad[q]
                .as_ref()
                .is_some_and(|s| s.boarding.is_some() && s.carrier.is_some_and(|c| p.members.contains(&c)))
    });
    let walking = p.dismount
        && p.members
            .iter()
            .any(|&k| w.can_move(k) && !w.mounted(k) && w.ai[k].as_ref().is_some_and(|a| a.order != Order::Idle));
    ramp || boarding || walking
}

/// Is the wait / branch condition set met in the side's picture (fog of war: picture only)?
pub fn set_met(pic: &ai::picture::PerceivedWorld, set: &ConditionSet, signs: &[f64], since: u64, tick: u64) -> bool {
    let one = |(i, c): (usize, &TriggerCondition)| match c {
        TriggerCondition::Elapsed { s } => (tick.saturating_sub(since)) as f64 * DT >= *s,
        // Sent after the platoon began to wait: a word is a transmission, not a state.
        TriggerCondition::CodeWord { word } => pic.code_words.iter().any(|c| c.word == *word && c.tick >= since),
        TriggerCondition::PlatoonAt { platoon, step } => {
            pic.platoon(*platoon).and_then(|v| v.plan.as_ref()).map_or_else(
                || pic.platoon(*platoon).is_some_and(|v| v.plan_done.contains(step)),
                |pl| pl.passed.contains(step) || (pl.cursor_id == *step && pl.waiting.is_some()),
            )
        }
        _ => control::condition_met(pic, c, signs.get(i).copied().unwrap_or(1.0)),
    };
    let mut it = set.conditions.iter().enumerate();
    if set.all { it.all(one) } else { set.conditions.is_empty() || it.any(one) }
}
