//! The reporting chain and each side's picture (DESIGN.md §6.2).
//!
//! Crew sightings become spot reports that reach the side picture after a doctrine-dependent
//! delay, with position error (small when lased, a fraction of range otherwise). Reports are
//! fused into contacts by proximity, so two crews reporting one enemy may briefly show two
//! contacts. Contacts age and expire. Own units are reported exactly (digital position
//! reporting; era-dependent lag is future work).

use ai::picture::{
    Contact, ContactId, ContactLevel, GunActivity, OwnUnit, PerceivedWorld, PlatoonView, ShotView, SideId,
    SquadPosture, SquadView, Task,
};
use rand_chacha::ChaCha8Rng;
use sim_data::{DoctrinePack, VehicleDef};

use crate::gunnery::Phase;
use crate::sensors::uniform;
use crate::world::{Order, Posture, World};
use crate::{DT, Event, Vec2, dist};

/// Correlation radius for fusing a report into an existing contact (m).
const FUSE_RADIUS: f64 = 150.0;
/// Re-report a tracked enemy at this interval (s).
const UPDATE_EVERY_S: f64 = 20.0;
/// Contacts disappear after this long without a report (s).
const EXPIRE_S: f64 = 180.0;
/// Shots stay in the picture this long (s) for display.
const SHOT_MEMORY_S: f64 = 4.0;
/// Bearing error of a spot report, 1σ (mrad).
const BEARING_SIGMA_MRAD: f64 = 3.0;

#[derive(Debug, Clone)]
pub struct PendingReport {
    pub deliver_tick: u64,
    pub observer: usize,
    pub target: usize,
    pub pos: Vec2,
    pub level: ContactLevel,
    pub type_name: Option<String>,
    pub range_m: f64,
}

pub struct SideState {
    pub picture: PerceivedWorld,
    pub pending: Vec<PendingReport>,
    pub next_contact: u32,
    pub doctrine: DoctrinePack,
    pub triggers: Vec<crate::control::Trigger>,
    pub next_graphic: u32,
}

pub fn gauss(rng: &mut ChaCha8Rng) -> f64 {
    // Box–Muller with libm for determinism.
    let u1 = uniform(rng).max(1e-12);
    let u2 = uniform(rng);
    libm::sqrt(-2.0 * libm::log(u1)) * libm::cos(std::f64::consts::TAU * u2)
}

impl SideState {
    pub fn new(side: SideId, doctrine: DoctrinePack) -> Self {
        Self {
            picture: PerceivedWorld::empty(side),
            pending: Vec::new(),
            next_contact: 0,
            doctrine,
            triggers: Vec::new(),
            next_graphic: 0,
        }
    }

    /// Queue a report of `target` seen by `observer`.
    #[allow(clippy::too_many_arguments)]
    pub fn queue(
        &mut self,
        world: &World,
        vehicles: &[VehicleDef],
        observer: usize,
        target: usize,
        level: ContactLevel,
        tick: u64,
        rng: &mut ChaCha8Rng,
    ) {
        let d = &self.doctrine;
        let (lo, hi) = (d.report_delay_min_s.get(), d.report_delay_max_s.get());
        let delay = lo + (hi - lo) * uniform(rng);
        let op = world.transform[observer].pos;
        let tp = world.transform[target].pos;
        let r = dist(op, tp);
        let lased = world.vehicle[observer].is_some_and(|vi| vehicles[vi].fire_control.laser_rangefinder);
        let fc = world.vehicle[observer].map(|vi| &vehicles[vi].fire_control);
        let range_sigma = match fc {
            Some(f) if lased => f.laser_range_error_m.get(),
            Some(f) => f.estimated_range_error_fraction.get() * r,
            None => 0.1 * r,
        };
        let br = libm::atan2(tp[0] - op[0], tp[1] - op[1]) + gauss(rng) * BEARING_SIGMA_MRAD / 1000.0;
        let rr = (r + gauss(rng) * range_sigma).max(10.0);
        let pos = [op[0] + libm::sin(br) * rr, op[1] + libm::cos(br) * rr];
        let type_name = (level == ContactLevel::Identified)
            .then(|| world.vehicle[target].map(|vi| vehicles[vi].name.clone()))
            .flatten();
        self.pending.push(PendingReport {
            deliver_tick: tick + (delay / DT) as u64,
            observer,
            target,
            pos,
            level,
            type_name,
            range_m: r,
        });
    }

    /// Deliver due reports into the picture. Returns events for this side.
    pub fn deliver(&mut self, world: &mut World, tick: u64) -> Vec<Event> {
        let mut events = Vec::new();
        let now = tick as f64 * DT;
        let due: Vec<PendingReport> = {
            let (due, keep): (Vec<_>, Vec<_>) = self.pending.drain(..).partition(|r| r.deliver_tick <= tick);
            self.pending = keep;
            due
        };
        for r in due {
            if !world.alive(r.observer) {
                continue; // the reporting crew was knocked out before it got the report off
            }
            // Prefer the contact this crew already reported for this track.
            let linked = world.sensors[r.observer]
                .as_ref()
                .and_then(|s| s.track(r.target))
                .and_then(|t| t.contact)
                .filter(|c| self.picture.contact(*c).is_some_and(|k| !k.destroyed));
            let fused = linked.or_else(|| {
                self.picture
                    .contacts
                    .iter()
                    .filter(|c| !c.destroyed && dist(c.pos, r.pos) < FUSE_RADIUS)
                    .min_by(|a, b| dist(a.pos, r.pos).total_cmp(&dist(b.pos, r.pos)))
                    .map(|c| c.id)
            });
            let reporter = world.id[r.observer];
            let (cid, new) = match fused {
                Some(cid) => {
                    let c = self.picture.contacts.iter_mut().find(|c| c.id == cid).expect("exists");
                    c.pos = r.pos;
                    c.last_report_s = now;
                    c.reported_by = reporter;
                    let upgraded = r.level > c.level;
                    c.level = c.level.max(r.level);
                    if r.type_name.is_some() {
                        c.type_name = r.type_name.clone();
                    }
                    (cid, upgraded)
                }
                None => {
                    self.next_contact += 1;
                    let cid = ContactId(self.next_contact);
                    self.picture.contacts.push(Contact {
                        id: cid,
                        pos: r.pos,
                        level: r.level,
                        type_name: r.type_name.clone(),
                        first_report_s: now,
                        last_report_s: now,
                        reported_by: reporter,
                        destroyed: false,
                    });
                    (cid, true)
                }
            };
            if let Some(tr) =
                world.sensors[r.observer].as_mut().and_then(|s| s.tracks.iter_mut().find(|t| t.target == r.target))
            {
                tr.contact = Some(cid);
            }
            if new {
                events.push(Event::SpotReport {
                    side: self.picture.side,
                    reporter,
                    contact: cid,
                    level: r.level,
                    new: fused.is_none(),
                    range_m: r.range_m,
                });
            }
        }
        self.picture.contacts.retain(|c| now - c.last_report_s < EXPIRE_S);
        events
    }

    /// Periodic update reports for enemies still in sight.
    pub fn queue_updates(&mut self, world: &mut World, vehicles: &[VehicleDef], tick: u64, rng: &mut ChaCha8Rng) {
        let every = (UPDATE_EVERY_S / DT) as u64;
        for o in 0..world.len() {
            if world.side[o] != self.picture.side || !world.alive(o) {
                continue;
            }
            let due: Vec<(usize, ContactLevel)> = world.sensors[o]
                .as_ref()
                .map(|s| {
                    s.tracks
                        .iter()
                        .filter(|t| t.last_seen_tick == tick && tick.saturating_sub(t.last_report_tick) >= every)
                        .map(|t| (t.target, t.level))
                        .collect()
                })
                .unwrap_or_default();
            for (t, lvl) in due {
                self.queue(world, vehicles, o, t, lvl, tick, rng);
                if let Some(tr) = world.sensors[o].as_mut().and_then(|s| s.tracks.iter_mut().find(|x| x.target == t)) {
                    tr.last_report_tick = tick;
                }
            }
        }
    }

    pub fn add_shot(&mut self, shot: ShotView) {
        self.picture.shots.push(shot);
    }

    /// Mark the contact nearest `pos` destroyed (an own crew saw its target brew up).
    pub fn mark_destroyed(&mut self, pos: Vec2) {
        if let Some(c) = self
            .picture
            .contacts
            .iter_mut()
            .filter(|c| !c.destroyed && dist(c.pos, pos) < FUSE_RADIUS * 2.0)
            .min_by(|a, b| dist(a.pos, pos).total_cmp(&dist(b.pos, pos)))
        {
            c.destroyed = true;
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn rebuild_own(
        &mut self,
        w: &World,
        vehicles: &[VehicleDef],
        squads: &[sim_data::SquadDef],
        round_labels: &[Vec<&'static str>],
        platoons: &[PlatoonView],
        tick: u64,
    ) {
        let now = tick as f64 * DT;
        let p = &mut self.picture;
        p.tick = tick;
        p.time_s = now;
        p.shots.retain(|s| now - s.time_s < SHOT_MEMORY_S);
        p.platoons = platoons.to_vec();
        p.own.clear();
        for k in 0..w.len() {
            if w.side[k] != p.side {
                continue;
            }
            let t = &w.transform[k];
            let (speed, fuel_fraction, name, vid) = match (&w.mobility[k], w.vehicle[k]) {
                (Some(m), Some(vi)) => {
                    let d = &vehicles[vi];
                    (m.speed, m.fuel_l / d.mobility.fuel_capacity_l.get(), d.name.clone(), d.id.clone())
                }
                _ => (0.0, 0.0, String::new(), String::new()),
            };
            let (mut task, route, note) = match w.ai[k].as_ref() {
                Some(ai) => match &ai.order {
                    Order::Move { route, segment, .. } => {
                        let mut r = vec![t.pos];
                        r.extend_from_slice(&route[(*segment + 1).min(route.len())..]);
                        let remaining = r.windows(2).map(|p| dist(p[0], p[1])).sum();
                        (Task::Moving { remaining_m: remaining }, r, ai.position_note.clone())
                    }
                    _ if ai.set => (Task::Set, Vec::new(), ai.position_note.clone()),
                    _ => {
                        let mut r = Vec::new();
                        if let Some(Order::Move { route, segment, .. }) = &ai.held {
                            r.push(t.pos);
                            r.extend_from_slice(&route[(*segment + 1).min(route.len())..]);
                        }
                        (Task::Idle, r, ai.position_note.clone())
                    }
                },
                None => (Task::Idle, Vec::new(), None),
            };
            let (turret, ammo) = w.gun[k].as_ref().map_or((t.heading, 0), |g| (g.turret, g.rounds()));
            let (rounds, ammo_selected) = match (w.vehicle[k], w.gun[k].as_ref()) {
                (Some(vi), Some(g)) => (
                    round_labels[vi].iter().zip(&g.stock).map(|(&l, &n)| (l.to_owned(), n)).collect(),
                    g.selected.map(|k| k.label().to_owned()),
                ),
                _ => (Vec::new(), None),
            };
            let establishment = w.vehicle[k].map_or(0, |vi| vehicles[vi].crew);
            let lost = w.gun[k].as_ref().map_or(0, |g| g.crew_loss);
            let crew = (establishment.saturating_sub(lost), establishment);
            let gun = match w.gun[k].as_ref().map(|g| g.phase) {
                Some(Phase::Laying { .. }) => GunActivity::Laying,
                Some(Phase::Lasing { .. }) => GunActivity::Lasing,
                Some(Phase::Reloading { .. }) => GunActivity::Reloading,
                _ => GunActivity::Ready,
            };
            let target =
                w.gun[k].as_ref().and_then(|g| g.target).and_then(|tg| w.sensors[k].as_ref()?.track(tg)?.contact);
            let sights_degraded = w.sensors[k].as_ref().is_some_and(|s| s.degraded);
            if w.gun[k].as_ref().is_some_and(|g| g.target.is_some()) {
                task = Task::Engaging;
            }
            p.own.push(OwnUnit {
                id: w.id[k],
                callsign: w.callsign[k].clone(),
                vehicle_id: vid,
                vehicle_name: name,
                platoon: w.platoon[k],
                pos: t.pos,
                heading: t.heading,
                turret,
                speed,
                fuel_fraction,
                ammo,
                full_load: w.gun[k].as_ref().map_or(0, |g| g.capacity.iter().sum()),
                service: None,
                rounds,
                ammo_selected,
                era_left: w.era[k],
                condition: w.condition[k],
                weapons: w.weapons[k],
                task,
                route,
                held: w.ai[k].as_ref().is_some_and(|a| a.held.is_some()),
                speed_limit: w.ai[k].as_ref().and_then(|a| match (&a.order, &a.held) {
                    (Order::Move { speed_limit, platoon_slot: false, .. }, _) => *speed_limit,
                    (_, Some(Order::Move { speed_limit, .. })) => *speed_limit,
                    _ => None,
                }),
                position_note: note,
                crew,
                sights_degraded,
                gun,
                target,
                smoke_salvos: w.smoke[k].salvos,
                engine_smoke: w.vehicle[k]
                    .filter(|&vi| vehicles[vi].engine_smoke.is_some())
                    .map(|_| w.smoke[k].engine_on),
                suppression: w.suppression[k],
                squad: w.squad[k].as_ref().map(|s| {
                    let d = &squads[s.def];
                    let posture = if s.mounted_in.is_some() {
                        SquadPosture::Mounted
                    } else if s.in_building.is_some() {
                        SquadPosture::InBuilding
                    } else {
                        match s.posture {
                            Posture::Moving => SquadPosture::Moving,
                            Posture::Halted => SquadPosture::Halted,
                            Posture::DugIn => SquadPosture::DugIn,
                        }
                    };
                    SquadView {
                        type_id: d.id.clone(),
                        type_name: d.name.clone(),
                        strength: (s.strength, s.men),
                        posture,
                        carrier: s.carrier.map(|c| w.id[c]),
                        building: s.in_building,
                        ineffective: s.ineffective,
                        bursts: s.weapons.iter().map(|x| x.bursts).sum(),
                    }
                }),
            });
        }
    }
}
