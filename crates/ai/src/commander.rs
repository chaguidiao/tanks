//! Commander-level AI (DESIGN.md §9.1, 0.2 Hz): turns a scenario plan into platoon orders using
//! the side's doctrine-pack templates. Reads only the side's `PerceivedWorld`.
//!
//! Attack template (generic OPFOR, after FM 100-2-1 (1984) ch. 5): platoons advance along the
//! axis in march column; at the pre-battle distance from the objective (or on contact) they
//! deploy abreast in pre-battle formation; at the battle distance (or under fire) they shake out
//! into battle formation and assault through the objective, then consolidate on it. A platoon
//! that has lost the doctrine's break-off fraction stops and fights in place.
//!
//! Defend template (ATP 3-20.15 ch. 5): occupy the battle position oriented on the threat.
//!
//! Orders are issued only on phase changes, so the radio order delay applies once per change.

use sim_data::{DoctrinePack, Formation, Mission, ObjectiveDef, Technique};

use crate::picture::{BatteryStatus, Condition, PerceivedWorld, PlatoonId, Vec2};

#[derive(Debug, Clone, PartialEq)]
pub enum Order {
    Move { platoon: PlatoonId, destination: Vec2, formation: Formation, technique: Technique },
    Occupy { platoon: PlatoonId, centre: Vec2, radius: f64, facing: f64 },
    Halt { platoon: PlatoonId },
    CallFire { battery: u32, at: Vec2, volleys: u32, smoke: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase {
    Waiting,
    March { waypoint: usize },
    PreBattle,
    Battle,
    Consolidate,
    Defending,
    BrokenOff,
}

#[derive(Debug, Clone)]
pub struct TaskState {
    pub platoons: Vec<PlatoonId>,
    pub mission: Mission,
    pub start_s: f64,
    pub phase: Phase,
    /// Per platoon: broken off.
    pub broken: Vec<bool>,
}

#[derive(Debug, Clone)]
pub struct Commander {
    pub tasks: Vec<TaskState>,
    /// Time of the last call for fire (s).
    pub last_call_s: f64,
}

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

    /// One decision step. Returns new orders.
    pub fn step(&mut self, pic: &PerceivedWorld, d: &DoctrinePack, objectives: &[ObjectiveDef]) -> Vec<Order> {
        let mut out = Vec::new();
        let fresh: Vec<Vec2> =
            pic.live_contacts().filter(|c| pic.time_s - c.last_report_s < 30.0).map(|c| c.pos).collect();
        let under_fire = pic.shots.iter().any(|s| !s.own);
        if let Some(o) = self.call_fire(pic, d) {
            out.push(o);
        }
        for t in self.tasks.iter_mut() {
            if pic.time_s < t.start_s {
                continue;
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
            match t.mission.clone() {
                Mission::Defend { centre, radius_m, facing_deg } => {
                    if t.phase != Phase::Defending {
                        t.phase = Phase::Defending;
                        let n = active.len() as f64;
                        for (k, (_, p)) in active.iter().enumerate() {
                            // Several platoons share the BP side by side across the threat axis.
                            let f = facing_deg.to_radians();
                            let off = (k as f64 - (n - 1.0) / 2.0) * radius_m * 1.6;
                            let c = crate::formation::slot_position(centre, f, (off, 0.0));
                            out.push(Order::Occupy { platoon: *p, centre: c, radius: radius_m, facing: f });
                        }
                    }
                }
                Mission::Attack { axis, objective } => {
                    let Some(obj) = objectives.iter().find(|o| o.name == objective) else { continue };
                    let lead = active
                        .iter()
                        .filter_map(|(_, p)| Self::centroid(pic, *p).map(|c| c.0))
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
                                if dist(lead, wp) < 250.0 && waypoint + 1 < axis.len() {
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
                    for (k, (_, p)) in active.iter().enumerate() {
                        let lateral = k as f64 - (n - 1.0) / 2.0;
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
}
