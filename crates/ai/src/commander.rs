//! Commander-level AI (DESIGN.md §9.1, 0.2 Hz): turns a scenario plan into platoon orders using
//! the side's doctrine-pack templates. Reads only the side's `PerceivedWorld`.
//!
//! Attack template (generic OPFOR, after FM 100-2-1 (1984) ch. 5): platoons advance along the
//! axis in march column; at the pre-battle distance from the objective (or on contact) they
//! deploy abreast in pre-battle formation; at the battle distance (or under fire) they shake out
//! into battle formation and assault through the objective, then consolidate on it. A platoon
//! that has lost the doctrine's break-off fraction stops and fights in place.
//!
//! Attack position (FM 3-90 ch. 3; FM 100-2-1 ch. 5): with an `assault` doctrine block the
//! platoons halt at an assault position and shake out together, instead of arriving one by one.
//!
//! Defend template (ATP 3-20.15 ch. 5): occupy the battle position oriented on the threat; with
//! alternate positions the platoons displace to the next one when a fresh contact closes on the
//! current one or the task has lost too many vehicles.
//!
//! Reserve (FM 3-90): hold an assembly position; committed as a counter-attack on the objective
//! when the objective is contested or the side has lost too much.
//!
//! Reconnaissance (FM 3-98: gain and maintain enemy contact): move ahead along an axis, stop at
//! the first contact and observe. Tasks may be held until a contact exists (`release_on_contact`).
//!
//! Orders are issued only on phase changes, so the radio order delay applies once per change.

use sim_data::{BattlePosition, DoctrinePack, Formation, Mission, ObjectiveDef, Technique};

use crate::picture::{
    BatteryStatus, Condition, PerceivedWorld, Plan, PlanStep, PlatoonId, PositionKind, SquadPosture, StepAction, Vec2,
};

#[derive(Debug, Clone, PartialEq)]
pub enum Order {
    Move {
        platoon: PlatoonId,
        destination: Vec2,
        formation: Formation,
        technique: Technique,
    },
    Occupy {
        platoon: PlatoonId,
        centre: Vec2,
        radius: f64,
        facing: f64,
    },
    Halt {
        platoon: PlatoonId,
    },
    CallFire {
        battery: u32,
        at: Vec2,
        volleys: u32,
        smoke: bool,
    },
    /// The carrier platoon's squads dismount where they are.
    Dismount {
        platoon: PlatoonId,
    },
    /// A checkpoint plan (dismount platoons, and carriers that must pick up their squads).
    Plan {
        platoon: PlatoonId,
        plan: Plan,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase {
    Waiting,
    March {
        waypoint: usize,
    },
    PreBattle,
    Battle,
    Consolidate,
    Defending,
    BrokenOff,
    /// Reserve holding its assembly position.
    Reserve,
    /// Reconnaissance observing from its position.
    Observing,
}

#[derive(Debug, Clone)]
pub struct TaskState {
    pub platoons: Vec<PlatoonId>,
    pub mission: Mission,
    pub start_s: f64,
    pub phase: Phase,
    /// Per platoon: broken off.
    pub broken: Vec<bool>,
    /// Held until the side's picture shows a contact (or `latest_s`).
    pub release_on_contact: bool,
    pub latest_s: Option<f64>,
    pub released: bool,
    /// Defend: index of the current position (0 = primary, then the alternates).
    pub bp_index: usize,
    /// Time the current position was ordered (s).
    pub bp_since_s: f64,
    /// Fraction of the task's vehicles lost at the last displacement (losses re-trigger only when
    /// they grow).
    pub displace_loss_mark: f64,
    /// Attack: assault position per platoon while the task closes up, and when the first
    /// platoon reported in position.
    pub assault_pos: Vec<(PlatoonId, Vec2)>,
    pub sync_since_s: Option<f64>,
    /// Carrier platoons of the task whose squads it has put on the ground: they overwatch
    /// instead of assaulting (plan M8F1-INFANTRY-AI §4).
    pub dismounted: Vec<PlatoonId>,
    /// Plans for dismount platoons, sent once their squads are out: (carrier platoon, plan,
    /// squads seen aboard since the plan was made).
    pub squad_plans: Vec<(PlatoonId, Plan, bool)>,
}

impl TaskState {
    pub fn new(
        platoons: Vec<PlatoonId>,
        mission: Mission,
        start_s: f64,
        release_on_contact: bool,
        latest_s: Option<f64>,
    ) -> Self {
        Self {
            platoons,
            mission,
            start_s,
            phase: Phase::Waiting,
            broken: Vec::new(),
            release_on_contact,
            latest_s,
            released: !release_on_contact,
            bp_index: 0,
            bp_since_s: 0.0,
            displace_loss_mark: 0.0,
            assault_pos: Vec::new(),
            sync_since_s: None,
            dismounted: Vec::new(),
            squad_plans: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Commander {
    pub tasks: Vec<TaskState>,
    /// Time of the last call for fire (s).
    pub last_call_s: f64,
}

/// Defending squads take buildings this far beyond the battle position's radius (m; plan
/// M8F1-INFANTRY-AI §5: within about 300 m of the position).
const SQUAD_BUILDINGS_M: f64 = 150.0;
/// Carriers are sent to their dismount point when this much short of it (m).
const DISMOUNT_LEAD_M: f64 = 400.0;

fn dist(a: Vec2, b: Vec2) -> f64 {
    libm::sqrt((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2))
}

impl Commander {
    pub fn new(tasks: Vec<TaskState>) -> Self {
        Self { tasks, last_call_s: f64::NEG_INFINITY }
    }

    fn centroid(pic: &PerceivedWorld, p: PlatoonId) -> Option<(Vec2, usize, usize)> {
        let pv = pic.platoon(p)?;
        let alive: Vec<_> =
            pv.members.iter().filter_map(|m| pic.unit(*m)).filter(|u| u.condition != Condition::Destroyed).collect();
        if alive.is_empty() {
            return None;
        }
        let n = alive.len() as f64;
        let c = [alive.iter().map(|u| u.pos[0]).sum::<f64>() / n, alive.iter().map(|u| u.pos[1]).sum::<f64>() / n];
        Some((c, alive.len(), pv.members.len()))
    }

    /// Fires on the freshest contact that is in range of a ready battery and not danger close
    /// (ATP 3-09.42: fires on targets that threaten the scheme of manoeuvre).
    fn call_fire(&mut self, pic: &PerceivedWorld, d: &DoctrinePack) -> Option<Order> {
        let fs = d.fire_support.as_ref()?;
        if pic.time_s - self.last_call_s < fs.repeat_s.get() {
            return None;
        }
        let mine: Vec<Vec2> = pic.own.iter().filter(|u| u.condition != Condition::Destroyed).map(|u| u.pos).collect();
        let mut best: Option<(u32, Vec2, f64)> = None;
        for b in pic.fire_support.iter().filter(|b| b.status == BatteryStatus::Ready && b.he_rounds > 0) {
            for c in pic.live_contacts().filter(|c| pic.time_s - c.last_report_s <= fs.max_contact_age_s.get()) {
                let r = dist(b.pos, c.pos);
                if r < b.min_range_m || r > b.max_range_m {
                    continue;
                }
                let nearest_own = mine.iter().map(|p| dist(*p, c.pos)).fold(f64::INFINITY, f64::min);
                if nearest_own < fs.danger_close_m.get() {
                    continue;
                }
                // Prefer the target closest to our own forces: it is the one that matters most.
                if best.is_none_or(|(_, _, n)| nearest_own < n) {
                    best = Some((b.id, c.pos, nearest_own));
                }
            }
        }
        let (battery, at, _) = best?;
        self.last_call_s = pic.time_s;
        Some(Order::CallFire { battery, at, volleys: fs.volleys, smoke: false })
    }

    /// Smoke on the nearest fresh contact as the assault deploys, from a ready battery with smoke
    /// in range (blinds the defenders' observation; not on top of own forces).
    fn smoke_mission(last_call_s: &mut f64, pic: &PerceivedWorld, d: &DoctrinePack, lead: Vec2) -> Option<Order> {
        let fs = d.fire_support.as_ref().filter(|f| f.smoke_volleys > 0)?;
        let mine: Vec<Vec2> = pic.own.iter().filter(|u| u.condition != Condition::Destroyed).map(|u| u.pos).collect();
        let target = pic
            .live_contacts()
            .filter(|c| pic.time_s - c.last_report_s <= fs.max_contact_age_s.get())
            .filter(|c| mine.iter().all(|p| dist(*p, c.pos) >= fs.danger_close_m.get()))
            .min_by(|a, b| dist(a.pos, lead).total_cmp(&dist(b.pos, lead)))?;
        let b = pic.fire_support.iter().find(|b| {
            b.status == BatteryStatus::Ready
                && b.smoke_rounds > 0
                && (b.min_range_m..=b.max_range_m).contains(&dist(b.pos, target.pos))
        })?;
        *last_call_s = pic.time_s;
        Some(Order::CallFire { battery: b.id, at: target.pos, volleys: fs.smoke_volleys, smoke: true })
    }

    /// Occupy `pos` with the active platoons side by side across the threat axis.
    fn occupy_all(out: &mut Vec<Order>, active: &[(usize, PlatoonId)], pos: &BattlePosition) {
        for (p, c, f) in Self::bp_slots(active, pos) {
            out.push(Order::Occupy { platoon: p, centre: c, radius: pos.radius_m, facing: f });
        }
    }

    fn bp_slots(active: &[(usize, PlatoonId)], pos: &BattlePosition) -> Vec<(PlatoonId, Vec2, f64)> {
        let n = active.len() as f64;
        let f = pos.facing_deg.to_radians();
        active
            .iter()
            .enumerate()
            .map(|(k, (_, p))| {
                let off = (k as f64 - (n - 1.0) / 2.0) * pos.radius_m * 1.6;
                (*p, crate::formation::slot_position(pos.centre, f, (off, 0.0)), f)
            })
            .collect()
    }

    /// The dismount platoon of carrier platoon `p`, and whether any of its squads ride (picture
    /// only: own units).
    fn troops(pic: &PerceivedWorld, p: PlatoonId) -> Option<(PlatoonId, bool)> {
        let pv = pic.platoon(p)?;
        let squads: Vec<_> = pic
            .own
            .iter()
            .filter(|u| u.condition != Condition::Destroyed)
            .filter(|u| u.squad.as_ref().and_then(|s| s.carrier).is_some_and(|c| pv.members.contains(&c)))
            .collect();
        let dp = squads.iter().find_map(|u| u.platoon)?;
        let aboard = squads.iter().any(|u| u.squad.as_ref().is_some_and(|s| s.posture == SquadPosture::Mounted));
        Some((dp, aboard))
    }

    /// Defend with IFVs (plan M8F1-INFANTRY-AI §4; ATP 3-21.71 (2010) ch. 3): the carriers drive
    /// to their slot of the battle position, dismount there and take it; the squads, once out,
    /// move to the house (or ground) at the front of the position. With `mount`, the squads first
    /// walk back and board where the carriers stand (displacement to the next position).
    #[allow(clippy::too_many_arguments)]
    fn defend_with_troops(
        t: &mut TaskState,
        out: &mut Vec<Order>,
        p: PlatoonId,
        c: Vec2,
        pos: &BattlePosition,
        f: f64,
        here: Option<Vec2>,
    ) {
        let mut steps = Vec::new();
        if let Some(h) = here {
            let mut s = PlanStep::new(1, h);
            s.actions = vec![StepAction::Mount];
            steps.push(s);
        }
        let mut s = PlanStep::new(steps.len() as u16 + 1, c);
        s.actions = vec![
            StepAction::Dismount,
            StepAction::OccupyBp { radius_m: pos.radius_m, facing: f, kind: PositionKind::HullDown },
        ];
        steps.push(s);
        out.push(Order::Plan { platoon: p, plan: Plan { steps } });
        let mut sq = PlanStep::new(1, c);
        sq.actions = vec![StepAction::OccupyBuildings { radius_m: pos.radius_m + SQUAD_BUILDINGS_M, facing: f }];
        t.squad_plans.retain(|(q, ..)| *q != p);
        t.squad_plans.push((p, Plan { steps: vec![sq] }, here.is_none()));
        if !t.dismounted.contains(&p) {
            t.dismounted.push(p);
        }
    }

    /// Fraction of the given platoons' vehicles that are no longer fit to fight.
    fn lost_fraction(pic: &PerceivedWorld, platoons: &[PlatoonId]) -> f64 {
        let (mut lost, mut all) = (0usize, 0usize);
        for &p in platoons {
            let Some(pv) = pic.platoon(p) else { continue };
            for m in &pv.members {
                all += 1;
                if pic.unit(*m).is_none_or(|u| matches!(u.condition, Condition::Destroyed | Condition::FirepowerKill)) {
                    lost += 1;
                }
            }
        }
        if all == 0 { 0.0 } else { lost as f64 / all as f64 }
    }

    /// One decision step. Returns new orders.
    pub fn step(&mut self, pic: &PerceivedWorld, d: &DoctrinePack, objectives: &[ObjectiveDef]) -> Vec<Order> {
        let mut out = Vec::new();
        let fresh: Vec<Vec2> =
            pic.live_contacts().filter(|c| pic.time_s - c.last_report_s < 30.0).map(|c| c.pos).collect();
        let under_fire = pic.shots.iter().any(|s| !s.own);
        if let Some(o) = self.call_fire(pic, d) {
            out.push(o);
        }
        // Side-wide losses, for committing the reserve.
        let side_lost = {
            let all: Vec<_> = pic.own.iter().collect();
            let lost = all.iter().filter(|u| matches!(u.condition, Condition::Destroyed | Condition::FirepowerKill));
            if all.is_empty() { 0.0 } else { lost.count() as f64 / all.len() as f64 }
        };
        for t in self.tasks.iter_mut() {
            if pic.time_s < t.start_s {
                continue;
            }
            if !t.released {
                // FM 3-98: the main body moves once reconnaissance has found the enemy.
                if pic.live_contacts().next().is_some() || t.latest_s.is_some_and(|l| pic.time_s >= l) {
                    t.released = true;
                } else {
                    continue;
                }
            }
            if t.broken.len() != t.platoons.len() {
                t.broken = vec![false; t.platoons.len()];
            }
            // Break-off on losses.
            for (i, &p) in t.platoons.iter().enumerate() {
                if t.broken[i] {
                    continue;
                }
                let lost = match Self::centroid(pic, p) {
                    Some((_, alive, total)) => 1.0 - alive as f64 / total as f64,
                    None => 1.0,
                };
                if lost >= d.break_off_losses.get() && matches!(t.mission, Mission::Attack { .. }) {
                    t.broken[i] = true;
                    out.push(Order::Halt { platoon: p });
                }
            }
            let active: Vec<(usize, PlatoonId)> =
                t.platoons.iter().copied().enumerate().filter(|(i, _)| !t.broken[*i]).collect();
            if active.is_empty() {
                t.phase = Phase::BrokenOff;
                continue;
            }
            // Squad plans go out once the squads are on the ground (after the ramp drill).
            let mut keep = Vec::new();
            for (p, plan, seen) in std::mem::take(&mut t.squad_plans) {
                match Self::troops(pic, p) {
                    Some((_, true)) => keep.push((p, plan, true)),
                    Some((dp, false)) if seen => out.push(Order::Plan { platoon: dp, plan }),
                    Some(_) => keep.push((p, plan, false)),
                    None => {}
                }
            }
            t.squad_plans = keep;
            match t.mission.clone() {
                Mission::Defend { centre, radius_m, facing_deg, alternates } => {
                    let position = |i: usize| -> BattlePosition {
                        if i == 0 { BattlePosition { centre, radius_m, facing_deg } } else { alternates[i - 1].clone() }
                    };
                    if t.phase != Phase::Defending {
                        t.phase = Phase::Defending;
                        t.bp_index = 0;
                        // The hold time only spaces displacements; the first one may come at once.
                        t.bp_since_s = f64::NEG_INFINITY;
                        let pos = position(0);
                        for (p, c, f) in Self::bp_slots(&active, &pos) {
                            match Self::troops(pic, p) {
                                Some((_, true)) => Self::defend_with_troops(t, &mut out, p, c, &pos, f, None),
                                _ => out.push(Order::Occupy { platoon: p, centre: c, radius: pos.radius_m, facing: f }),
                            }
                        }
                    } else if let Some(def) = d.defence.as_ref()
                        && t.bp_index < alternates.len()
                        && pic.time_s - t.bp_since_s >= def.displace_hold_s.get()
                    {
                        // ATP 3-20.15 ch. 5: displace to the next position on the trigger, before the
                        // platoon is fixed or overrun in the primary position.
                        let here = position(t.bp_index);
                        let close = fresh.iter().any(|c| dist(*c, here.centre) < def.displace_contact_range_m.get());
                        let lost = Self::lost_fraction(pic, &t.platoons);
                        if close || (lost >= def.displace_losses.get() && lost > t.displace_loss_mark) {
                            t.bp_index += 1;
                            t.bp_since_s = pic.time_s;
                            t.displace_loss_mark = lost;
                            let pos = position(t.bp_index);
                            for (p, c, f) in Self::bp_slots(&active, &pos) {
                                let here = Self::centroid(pic, p).map(|x| x.0);
                                match Self::troops(pic, p) {
                                    Some((_, aboard)) if t.dismounted.contains(&p) || aboard => {
                                        let mount = if aboard { None } else { here };
                                        Self::defend_with_troops(t, &mut out, p, c, &pos, f, mount);
                                    }
                                    _ => out.push(Order::Occupy {
                                        platoon: p,
                                        centre: c,
                                        radius: pos.radius_m,
                                        facing: f,
                                    }),
                                }
                            }
                        }
                    }
                }
                Mission::Reserve { assembly, objective } => {
                    if t.phase != Phase::Reserve {
                        t.phase = Phase::Reserve;
                        Self::occupy_all(&mut out, &active, &assembly);
                    } else if let Some(def) = d.defence.as_ref()
                        && let Some(obj) = objectives.iter().find(|o| o.name == objective)
                    {
                        let contested = fresh
                            .iter()
                            .any(|c| dist(*c, obj.centre) < obj.radius_m + def.reserve_commit_range_m.get());
                        if contested || side_lost >= def.reserve_commit_losses.get() {
                            // Counter-attack: from here on the task is an attack onto the objective.
                            t.mission = Mission::Attack { axis: Vec::new(), objective };
                            t.phase = Phase::Waiting;
                        }
                    }
                }
                Mission::Recon { axis, observe } => {
                    let lead = active.iter().filter_map(|(_, p)| Self::centroid(pic, *p).map(|c| c.0)).next();
                    let Some(lead) = lead else { continue };
                    match t.phase {
                        Phase::Observing => {}
                        Phase::March { waypoint } => {
                            // Stop at the first contact inside engagement range and observe it (FM 3-98:
                            // gain contact, do not become decisively engaged).
                            let contact = fresh
                                .iter()
                                .copied()
                                .filter(|c| dist(*c, lead) < d.engagement_range_m.get())
                                .min_by(|a, b| dist(*a, lead).total_cmp(&dist(*b, lead)));
                            if let Some(c) = contact {
                                t.phase = Phase::Observing;
                                let here = BattlePosition {
                                    centre: lead,
                                    radius_m: observe.radius_m.min(120.0),
                                    facing_deg: crate::drive::bearing(lead, c).to_degrees(),
                                };
                                Self::occupy_all(&mut out, &active, &here);
                            } else if dist(lead, axis.get(waypoint).copied().unwrap_or(observe.centre)) < 250.0 {
                                if waypoint + 1 < axis.len() {
                                    t.phase = Phase::March { waypoint: waypoint + 1 };
                                    Self::recon_move(&mut out, &active, axis[waypoint + 1], d);
                                } else {
                                    t.phase = Phase::Observing;
                                    Self::occupy_all(&mut out, &active, &observe);
                                }
                            }
                        }
                        _ => match axis.first() {
                            Some(&wp) => {
                                t.phase = Phase::March { waypoint: 0 };
                                Self::recon_move(&mut out, &active, wp, d);
                            }
                            None => {
                                t.phase = Phase::Observing;
                                Self::occupy_all(&mut out, &active, &observe);
                            }
                        },
                    }
                }
                Mission::Attack { axis, objective } => {
                    let Some(obj) = objectives.iter().find(|o| o.name == objective) else { continue };
                    // The task's squads on foot count as its lead too: the IFVs consolidate on the
                    // objective once their squads hold it.
                    let squads_out = active
                        .iter()
                        .filter(|(_, p)| t.dismounted.contains(p))
                        .filter_map(|(_, p)| Self::troops(pic, *p))
                        .filter(|(_, aboard)| !aboard)
                        .filter_map(|(dp, _)| Self::centroid(pic, dp).map(|c| c.0));
                    let lead = active
                        .iter()
                        .filter_map(|(_, p)| Self::centroid(pic, *p).map(|c| c.0))
                        .chain(squads_out)
                        .min_by(|a, b| dist(*a, obj.centre).total_cmp(&dist(*b, obj.centre)));
                    let Some(lead) = lead else { continue };
                    let d_obj = dist(lead, obj.centre);
                    let contact_near = |r: f64| fresh.iter().any(|c| dist(*c, lead) < r);
                    let want = if d_obj < obj.radius_m {
                        Phase::Consolidate
                    } else if d_obj < d.battle_distance_m.get()
                        || contact_near(d.battle_distance_m.get() + 500.0)
                        || under_fire
                    {
                        Phase::Battle
                    } else if d_obj < d.prebattle_distance_m.get() || contact_near(d.prebattle_distance_m.get()) {
                        Phase::PreBattle
                    } else {
                        match t.phase {
                            Phase::March { waypoint } => {
                                let wp = axis.get(waypoint).copied().unwrap_or(obj.centre);
                                // Past the last axis waypoint the march goes on to the objective (an axis
                                // that stops short of the pre-battle distance must not stall the attack).
                                if dist(lead, wp) < 250.0 && waypoint < axis.len() {
                                    Phase::March { waypoint: waypoint + 1 }
                                } else {
                                    Phase::March { waypoint }
                                }
                            }
                            _ => Phase::March { waypoint: 0 },
                        }
                    };
                    // Never step back from a deployed phase to the march.
                    let want = if t.phase >= Phase::PreBattle && want < t.phase && want != Phase::Consolidate {
                        t.phase
                    } else {
                        want
                    };
                    // Attack position: while closing up, hold the assault until every platoon is in
                    // position, then go together (the line of departure); the timeout since the first
                    // arrival releases it anyway, and so does coming under fire (FM 3-90 ch. 3).
                    let want = match (&d.assault, want, t.phase) {
                        (Some(a), Phase::PreBattle | Phase::Battle, Phase::PreBattle) if !under_fire => {
                            let in_position = |p: PlatoonId| -> bool {
                                let Some((c, _, _)) = Self::centroid(pic, p) else { return true };
                                // Already inside the assault position distance counts as there.
                                dist(c, obj.centre) <= a.assault_position_m.get() + a.in_position_m.get()
                                    || t.assault_pos
                                        .iter()
                                        .find(|(q, _)| *q == p)
                                        .is_some_and(|(_, ap)| dist(c, *ap) <= a.in_position_m.get())
                            };
                            let arrived = active.iter().filter(|(_, p)| in_position(*p)).count();
                            if arrived > 0 && t.sync_since_s.is_none() {
                                t.sync_since_s = Some(pic.time_s);
                            }
                            let late = t.sync_since_s.is_some_and(|s0| pic.time_s - s0 >= a.sync_timeout_s.get());
                            if arrived == active.len() || late { Phase::Battle } else { Phase::PreBattle }
                        }
                        _ => want,
                    };
                    // Dismount point (plan M8F1-INFANTRY-AI §4): the squads go on foot short of the
                    // objective, or when a fresh contact is as close; the carriers overwatch from
                    // there and move up only to consolidate.
                    let dm = d.infantry.dismount.attack_m.get();
                    for &(_, p) in &active {
                        if t.dismounted.contains(&p) || want == Phase::Consolidate {
                            continue;
                        }
                        let (Some((_, true)), Some((c, ..))) = (Self::troops(pic, p), Self::centroid(pic, p)) else {
                            continue;
                        };
                        // The carriers get a plan ending at the dismount point a little before they
                        // reach it, so the order is through and they stop on the point.
                        let to_obj = dist(c, obj.centre);
                        let at = dm + obj.radius_m;
                        if to_obj > at + DISMOUNT_LEAD_M && !fresh.iter().any(|f| dist(*f, c) <= dm) {
                            continue;
                        }
                        t.dismounted.push(p);
                        let point = if to_obj > at {
                            crate::formation::slot_position(
                                obj.centre,
                                crate::drive::bearing(c, obj.centre),
                                (0.0, -at),
                            )
                        } else {
                            c
                        };
                        let mut stop = PlanStep::new(1, point);
                        stop.actions = vec![StepAction::Dismount];
                        out.push(Order::Plan { platoon: p, plan: Plan { steps: vec![stop] } });
                        let edge = crate::formation::slot_position(
                            obj.centre,
                            crate::drive::bearing(c, obj.centre),
                            (0.0, -obj.radius_m * 0.5),
                        );
                        let mut s = PlanStep::new(1, edge);
                        s.actions = vec![StepAction::OccupyBuildings {
                            radius_m: obj.radius_m,
                            facing: crate::drive::bearing(c, obj.centre),
                        }];
                        t.squad_plans.push((p, Plan { steps: vec![s] }, true));
                    }
                    if want == t.phase {
                        continue;
                    }
                    t.phase = want;
                    if want == Phase::Battle
                        && let Some(o) = Self::smoke_mission(&mut self.last_call_s, pic, d, lead)
                    {
                        out.push(o);
                    }
                    let dir = crate::drive::bearing(lead, obj.centre);
                    let n = active.len() as f64;
                    t.assault_pos.clear();
                    t.sync_since_s = None;
                    for (k, (_, p)) in active.iter().enumerate() {
                        let lateral = k as f64 - (n - 1.0) / 2.0;
                        if t.dismounted.contains(p) && want != Phase::Consolidate {
                            continue; // overwatching its squads from the dismount point
                        }
                        match want {
                            Phase::March { waypoint } => {
                                let wp = axis.get(waypoint).copied().unwrap_or(obj.centre);
                                out.push(Order::Move {
                                    platoon: *p,
                                    destination: wp,
                                    formation: d.march_formation,
                                    technique: Technique::Traveling,
                                });
                            }
                            Phase::PreBattle | Phase::Battle => {
                                let (spread, f) = if want == Phase::PreBattle {
                                    (450.0, d.prebattle_formation)
                                } else {
                                    (300.0, d.battle_formation)
                                };
                                if want == Phase::PreBattle
                                    && let Some(a) = &d.assault
                                {
                                    // Close up on the assault position abreast; a platoon already
                                    // nearer to the objective than that stops where it is.
                                    let ap = crate::formation::slot_position(
                                        obj.centre,
                                        dir,
                                        (lateral * spread, -a.assault_position_m.get()),
                                    );
                                    let here = Self::centroid(pic, *p).map(|c| c.0);
                                    if here.is_some_and(|h| dist(h, obj.centre) <= a.assault_position_m.get()) {
                                        out.push(Order::Halt { platoon: *p });
                                    } else {
                                        t.assault_pos.push((*p, ap));
                                        out.push(Order::Move {
                                            platoon: *p,
                                            destination: ap,
                                            formation: f,
                                            technique: Technique::Traveling,
                                        });
                                    }
                                    continue;
                                }
                                // Assault through: aim beyond the objective centre.
                                let dest = crate::formation::slot_position(
                                    obj.centre,
                                    dir,
                                    (lateral * spread, obj.radius_m * 0.5),
                                );
                                out.push(Order::Move {
                                    platoon: *p,
                                    destination: dest,
                                    formation: f,
                                    technique: Technique::Traveling,
                                });
                            }
                            Phase::Consolidate => {
                                let c = crate::formation::slot_position(obj.centre, dir, (lateral * obj.radius_m, 0.0));
                                out.push(Order::Occupy {
                                    platoon: *p,
                                    centre: c,
                                    radius: obj.radius_m * 0.8,
                                    facing: dir,
                                });
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        out
    }

    /// Reconnaissance move: traveling overwatch in the march formation, platoons side by side.
    fn recon_move(out: &mut Vec<Order>, active: &[(usize, PlatoonId)], to: Vec2, d: &DoctrinePack) {
        let n = active.len() as f64;
        for (k, (_, p)) in active.iter().enumerate() {
            let off = (k as f64 - (n - 1.0) / 2.0) * 300.0;
            out.push(Order::Move {
                platoon: *p,
                destination: [to[0] + off, to[1]],
                formation: d.march_formation,
                technique: Technique::TravelingOverwatch,
            });
        }
    }
}
