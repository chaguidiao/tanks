//! The simulation core: deterministic, fixed 20 Hz tick, no graphics (CLAUDE.md rules 1–2).
//!
//! API boundary (DESIGN.md §4.2):
//! ```text
//! sim.submit(side, cmd)    orders in (radio delay; timestamped and logged for replay)
//! sim.step() -> events     advance one tick
//! sim.perceived(side)      what a side knows (players and AI read only this)
//! sim.truth()              ground truth: AAR, tests, debug-truth only
//! ```
//!
//! Tick pipeline: deliver orders → platoon AI (1 Hz) → vehicle AI (5 Hz) → sensors (4 Hz) →
//! reports → gunnery → impacts → mobility → pictures.

pub mod command;
pub mod gunnery;
pub mod mobility;
pub mod picture;
pub mod platoon;
pub mod platoon_ai;
pub mod sensors;
pub mod vehicle;
pub mod world;

use std::sync::Arc;

use ai::picture::{Condition, PlatoonId, ShotView, WeaponsStatus};
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;
use sim_data::{AmmoDef, DataSet, ScenarioDef, VehicleDef};
use terrain::BakedMap;
use terrain::ground::Ground;
use terrain::hash::StableHasher;
use terrain::path::Pathfinder;

pub use ai::picture::{self as view, PerceivedWorld};
pub use command::{Addressee, Command, Event, KillKind, PlatoonReport, PositionKind, TimedCommand};
pub use world::{EntityId, SideId, World};

pub type Vec2 = [f64; 2];

/// Simulation rate (DESIGN.md §4.1). Time compression runs more ticks, never a larger step.
pub const TICK_HZ: u32 = 20;
pub const DT: f64 = 1.0 / TICK_HZ as f64;
/// Vehicle AI decision rate: every 4th tick = 5 Hz.
pub const VEHICLE_AI_EVERY: u64 = 4;
/// Platoon AI rate: 1 Hz.
pub const PLATOON_AI_EVERY: u64 = 20;

/// One seeded ChaCha stream per subsystem (CLAUDE.md rule 2).
#[derive(Debug, Clone)]
pub struct Rngs {
    pub mobility: ChaCha8Rng,
    pub sensors: ChaCha8Rng,
    pub gunnery: ChaCha8Rng,
    pub ai: ChaCha8Rng,
    pub comms: ChaCha8Rng,
}

impl Rngs {
    fn new(seed: u64) -> Self {
        let s =
            |k: u64| ChaCha8Rng::seed_from_u64(terrain::noise::hash_u64(seed ^ k.wrapping_mul(0x9e37_79b9_7f4a_7c15)));
        Self { mobility: s(1), sensors: s(2), gunnery: s(3), ai: s(4), comms: s(5) }
    }
}

pub(crate) fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    libm::sqrt((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2))
}

pub(crate) fn wrap(a: f64) -> f64 {
    ai::drive::wrap(a)
}

pub struct Sim {
    tick: u64,
    map: Arc<BakedMap>,
    data: Arc<DataSet>,
    vehicles: Vec<VehicleDef>,
    /// Per vehicle definition: index into `ammo` of its main round.
    ammo_of: Vec<usize>,
    ammo: Vec<AmmoDef>,
    world: World,
    incoming: Vec<TimedCommand>,
    pending: Vec<(u64, TimedCommand)>,
    log: Vec<TimedCommand>,
    sides: Vec<picture::SideState>,
    side_names: Vec<String>,
    platoons: Vec<platoon::Platoon>,
    pathfinder: Pathfinder,
    rngs: Rngs,
    start_time_s: u32,
    los_cache: sensors::LosCache,
    projectiles: Vec<gunnery::Projectile>,
    pictures: Vec<PerceivedWorld>,
    pub(crate) scenario: ScenarioDef,
    ended: bool,
}

impl Sim {
    pub fn new(scenario: &ScenarioDef, data: Arc<DataSet>, map: Arc<BakedMap>) -> Result<Self, String> {
        let mut world = World::default();
        let mut vehicles: Vec<VehicleDef> = Vec::new();
        let mut ammo: Vec<AmmoDef> = Vec::new();
        let mut ammo_of = Vec::new();
        let mut platoons: Vec<platoon::Platoon> = Vec::new();
        let mut sides = Vec::new();
        let g = Ground::new(&map, &data.terrain);
        for (si, side) in scenario.sides.iter().enumerate() {
            let year = side.tech_year.unwrap_or(scenario.date.0);
            let doctrine = data
                .doctrine_pack(&side.doctrine)
                .ok_or_else(|| format!("side {}: unknown doctrine pack '{}'", side.name, side.doctrine))?
                .clone();
            for u in &side.units {
                let def = data
                    .vehicle(&u.vehicle)
                    .ok_or_else(|| format!("{}: unknown vehicle '{}'", u.callsign, u.vehicle))?;
                if !def.in_service.contains(year) {
                    return Err(format!("{}: {} is not in service in {year}", u.callsign, def.name));
                }
                if map.mobility(&def.id).is_none() {
                    return Err(format!("map has no mobility layer for '{}'; re-bake", def.id));
                }
                let vi = match vehicles.iter().position(|v| v.id == def.id) {
                    Some(i) => i,
                    None => {
                        vehicles.push(def.clone());
                        // Main round: the first load entry in service at the scenario date.
                        let round = def
                            .armament
                            .load
                            .iter()
                            .filter_map(|l| data.ammo(&l.ammo))
                            .find(|a| a.in_service.contains(year))
                            .or_else(|| def.armament.load.first().and_then(|l| data.ammo(&l.ammo)))
                            .ok_or_else(|| format!("{}: no ammunition", def.id))?;
                        let ai_ = match ammo.iter().position(|a| a.id == round.id) {
                            Some(i) => i,
                            None => {
                                ammo.push(round.clone());
                                ammo.len() - 1
                            }
                        };
                        ammo_of.push(ai_);
                        vehicles.len() - 1
                    }
                };
                let rounds: u32 = def.armament.load.iter().map(|l| l.count).sum();
                let z = g.height(u.position[0], u.position[1]);
                world.spawn_vehicle(
                    SideId(si as u8),
                    &u.callsign,
                    vi,
                    &vehicles[vi],
                    u.position,
                    z,
                    u.heading_deg.to_radians(),
                    year,
                    rounds,
                );
                let k = world.len() - 1;
                if let Some(pc) = &u.platoon {
                    let p = match platoons.iter_mut().find(|p| p.side.0 as usize == si && &p.callsign == pc) {
                        Some(p) => p,
                        None => {
                            platoons.push(platoon::Platoon {
                                id: PlatoonId(platoons.len() as u32 + 1),
                                side: SideId(si as u8),
                                callsign: pc.clone(),
                                members: Vec::new(),
                                formation: doctrine.default_formation,
                                technique: sim_data::Technique::Traveling,
                                order: platoon::PlatoonOrder::None,
                                heading: u.heading_deg.to_radians(),
                                replanned: 0,
                                contact_reported: false,
                                slot_targets: Vec::new(),
                                bp: Vec::new(),
                                resume: None,
                                quiet_since: 0,
                                last_strength: 0,
                            });
                            platoons.last_mut().expect("pushed")
                        }
                    };
                    p.members.push(k);
                    world.platoon[k] = Some(p.id);
                }
            }
            sides.push(picture::SideState::new(SideId(si as u8), doctrine));
        }
        let n_sides = scenario.sides.len();
        let mut sim = Self {
            tick: 0,
            map,
            data,
            vehicles,
            ammo_of,
            ammo,
            world,
            incoming: Vec::new(),
            pending: Vec::new(),
            log: Vec::new(),
            sides,
            side_names: scenario.sides.iter().map(|s| s.name.clone()).collect(),
            platoons,
            pathfinder: Pathfinder::new(),
            rngs: Rngs::new(scenario.seed),
            start_time_s: scenario.start_time_s,
            los_cache: sensors::LosCache::default(),
            projectiles: Vec::new(),
            pictures: (0..n_sides).map(|s| PerceivedWorld::empty(SideId(s as u8))).collect(),
            scenario: scenario.clone(),
            ended: false,
        };
        sim.update_pictures();
        Ok(sim)
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Seconds since scenario start.
    pub fn time_s(&self) -> f64 {
        self.tick as f64 * DT
    }

    /// Local time of day, seconds after midnight.
    pub fn clock_s(&self) -> f64 {
        self.start_time_s as f64 + self.time_s()
    }

    pub fn map(&self) -> &Arc<BakedMap> {
        &self.map
    }

    pub fn data(&self) -> &Arc<DataSet> {
        &self.data
    }

    pub fn scenario(&self) -> &ScenarioDef {
        &self.scenario
    }

    pub fn side_names(&self) -> &[String] {
        &self.side_names
    }

    pub fn vehicle_defs(&self) -> &[VehicleDef] {
        &self.vehicles
    }

    pub fn ended(&self) -> bool {
        self.ended
    }

    /// Queue an order over the radio net: it takes effect after the side's doctrinal order
    /// delay (DESIGN.md §8.2). Logged for replay.
    pub fn submit(&mut self, side: SideId, cmd: Command) {
        self.incoming.push(TimedCommand { tick: self.tick, side, cmd, immediate: false });
    }

    /// Pre-planned order (code word, trigger, planning phase): no radio delay.
    pub fn submit_immediate(&mut self, side: SideId, cmd: Command) {
        self.incoming.push(TimedCommand { tick: self.tick, side, cmd, immediate: true });
    }

    pub fn command_log(&self) -> &[TimedCommand] {
        &self.log
    }

    pub fn perceived(&self, side: SideId) -> &PerceivedWorld {
        &self.pictures[side.0 as usize]
    }

    /// Ground truth. Only for AAR, tests and the client's `debug-truth` feature.
    pub fn truth(&self) -> &World {
        &self.world
    }

    pub fn step(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        let tick = self.tick;

        // 1. Orders: draw radio delays in submission order, then deliver due orders.
        for c in std::mem::take(&mut self.incoming) {
            let delay = if c.immediate {
                0
            } else {
                let d = &self.sides[c.side.0 as usize].doctrine;
                let (lo, hi) = (d.order_delay_min_s.get(), d.order_delay_max_s.get());
                ((lo + (hi - lo) * sensors::uniform(&mut self.rngs.comms)) / DT) as u64
            };
            self.log.push(c.clone());
            self.pending.push((tick + delay, c));
        }
        let (due, keep): (Vec<_>, Vec<_>) =
            std::mem::take(&mut self.pending).into_iter().partition(|(t, _)| *t <= tick);
        self.pending = keep;
        for (_, c) in due {
            self.apply(&c, &mut events);
        }

        // 2. Platoon AI.
        if tick.is_multiple_of(PLATOON_AI_EVERY) {
            let mut ctx = platoon_ai::Ctx {
                world: &mut self.world,
                vehicles: &self.vehicles,
                map: &self.map,
                td: &self.data.terrain,
                pictures: &self.pictures,
                doctrines: &self.sides.iter().map(|s| s.doctrine.clone()).collect::<Vec<_>>(),
                pathfinder: &mut self.pathfinder,
                tick,
                events: &mut events,
            };
            platoon_ai::step(&mut self.platoons, &mut ctx);
        }

        // 3. Vehicle AI.
        if tick.is_multiple_of(VEHICLE_AI_EVERY) {
            vehicle::step(&mut self.world, &self.vehicles, &mut events);
        }

        // 4–5. Sensors and reports.
        if tick.is_multiple_of(sensors::SENSOR_EVERY) {
            let g = Ground::new(&self.map, &self.data.terrain);
            let sightings = sensors::step(
                &mut self.world,
                &self.vehicles,
                &g,
                &self.data.detection,
                self.start_time_s as f64 + tick as f64 * DT,
                tick,
                &mut self.rngs.sensors,
                &mut self.los_cache,
            );
            for s in sightings {
                let side = self.world.side[s.observer].0 as usize;
                self.sides[side].queue(
                    &self.world,
                    &self.vehicles,
                    s.observer,
                    s.target,
                    s.level,
                    tick,
                    &mut self.rngs.sensors,
                );
                if let Some(tr) = self.world.sensors[s.observer]
                    .as_mut()
                    .and_then(|x| x.tracks.iter_mut().find(|t| t.target == s.target))
                {
                    tr.last_report_tick = tick;
                }
            }
            for side in self.sides.iter_mut() {
                side.queue_updates(&mut self.world, &self.vehicles, tick, &mut self.rngs.sensors);
            }
        }
        for side in self.sides.iter_mut() {
            events.extend(side.deliver(&mut self.world, tick));
        }

        // 6. Gunnery.
        let engage: Vec<f64> = self.sides.iter().map(|s| s.doctrine.engagement_range_m.get()).collect();
        let fired = gunnery::step(
            &mut self.world,
            &self.vehicles,
            &self.ammo_of,
            &self.ammo,
            &self.data.penetration,
            &self.data.damage,
            &engage,
            tick,
            &mut self.rngs.gunnery,
        );
        for f in fired {
            let p = &f.projectile;
            let r = dist(p.from, p.to);
            events.push(Event::Fired { unit: self.world.id[f.shooter], range_m: r });
            let shooter_side = self.world.side[f.shooter];
            for side in self.sides.iter_mut() {
                let own = side.picture.side == shooter_side;
                // The other side sees the shot if it already has the shooter as a contact.
                let known = own || side.picture.contacts.iter().any(|c| !c.destroyed && dist(c.pos, p.from) < 300.0);
                if known {
                    side.add_shot(ShotView {
                        from: p.from,
                        to: p.to,
                        time_s: tick as f64 * DT,
                        hit: p.outcome.hit,
                        own,
                    });
                }
            }
            self.projectiles.push(f.projectile);
        }

        // 7. Impacts.
        let (hits, flying): (Vec<_>, Vec<_>) =
            std::mem::take(&mut self.projectiles).into_iter().partition(|p| p.impact_tick <= tick);
        self.projectiles = flying;
        for p in hits {
            let o = &p.outcome;
            let kill = if o.hit && self.world.alive(p.target) {
                gunnery::apply_damage(&mut self.world, p.target, o.kill, o.sights_damaged)
            } else {
                None
            };
            events.push(Event::ShotResult { unit: self.world.id[p.shooter], hit: o.hit, kill });
            if o.hit {
                events.push(Event::Hit { unit: self.world.id[p.target], penetrated: o.penetrated, kill });
            }
            if kill.is_some() && self.world.condition[p.target] == Condition::Destroyed {
                let s = self.world.side[p.shooter].0 as usize;
                self.sides[s].mark_destroyed(self.world.transform[p.target].pos);
            }
        }

        // 8. Mobility.
        mobility::step(&mut self.world, &self.vehicles, &self.map, &self.data.terrain, &mut events);

        self.tick += 1;
        self.update_pictures();
        events
    }

    fn platoon_mut(&mut self, side: SideId, id: PlatoonId) -> Option<&mut platoon::Platoon> {
        self.platoons.iter_mut().find(|p| p.id == id && p.side == side)
    }

    fn members(&self, side: SideId, to: Addressee) -> Vec<usize> {
        match to {
            Addressee::Unit(u) => self.world.index_of(u).filter(|&k| self.world.side[k] == side).into_iter().collect(),
            Addressee::Platoon(p) => self
                .platoons
                .iter()
                .find(|x| x.id == p && x.side == side)
                .map(|x| x.members.clone())
                .unwrap_or_default(),
        }
    }

    fn apply(&mut self, c: &TimedCommand, events: &mut Vec<Event>) {
        match &c.cmd {
            Command::Move { unit, destination, speed_limit_mps } => {
                let Some(k) = self.world.index_of(*unit) else { return };
                if self.world.side[k] != c.side || !self.world.can_move(k) {
                    return;
                }
                let Some(vi) = self.world.vehicle[k] else { return };
                // A single-vehicle order detaches it from its platoon's current task.
                if let Some(pid) = self.world.platoon[k]
                    && let Some(p) = self.platoon_mut(c.side, pid)
                {
                    p.order = platoon::PlatoonOrder::None;
                }
                let layer = self.map.mobility(&self.vehicles[vi].id).expect("checked at spawn");
                let pos = self.world.transform[k].pos;
                let cell = self.map.manifest.cell_size_m;
                match self.pathfinder.find(&layer.speed, cell, pos, *destination) {
                    Some(route) => {
                        let length = route.windows(2).map(|w| dist(w[0], w[1])).sum::<f64>();
                        self.world.set_route(k, route, *speed_limit_mps, None, false, ai::drive::ARRIVAL_RADIUS);
                        events.push(Event::RoutePlanned { unit: *unit, length_m: length });
                    }
                    None => events.push(Event::NoRoute { unit: *unit }),
                }
            }
            Command::Halt { to } => {
                for k in self.members(c.side, *to) {
                    self.world.halt(k);
                    events.push(Event::Halted { unit: self.world.id[k] });
                }
                if let Addressee::Platoon(p) = to
                    && let Some(p) = self.platoon_mut(c.side, *p)
                {
                    p.order = platoon::PlatoonOrder::None;
                }
            }
            Command::WeaponsControl { to, status } => {
                for k in self.members(c.side, *to) {
                    self.world.weapons[k] = *status;
                }
                events.push(Event::OrderReceived { side: c.side, to: *to });
            }
            Command::Engage { unit, contact } => {
                let Some(k) = self.world.index_of(*unit).filter(|&k| self.world.side[k] == c.side) else { return };
                // Lay on the crew's own track nearest the reported contact position.
                let Some(cpos) = self.pictures[c.side.0 as usize].contact(*contact).map(|x| x.pos) else { return };
                let best = self.world.sensors[k].as_ref().and_then(|s| {
                    s.tracks
                        .iter()
                        .min_by(|a, b| {
                            dist(self.world.transform[a.target].pos, cpos)
                                .total_cmp(&dist(self.world.transform[b.target].pos, cpos))
                        })
                        .map(|t| t.target)
                });
                if let Some(g) = self.world.gun[k].as_mut() {
                    g.assigned = best;
                }
                if self.world.weapons[k] == WeaponsStatus::Hold {
                    self.world.weapons[k] = WeaponsStatus::Tight;
                }
            }
            Command::PlatoonMove { platoon, destination, formation, technique } => {
                let (dest, f, t) = (*destination, *formation, *technique);
                if let Some(p) = self.platoon_mut(c.side, *platoon) {
                    if let Some(f) = f {
                        p.formation = f;
                    }
                    p.technique = t;
                    p.order = platoon::PlatoonOrder::Move {
                        destination: dest,
                        route: Vec::new(),
                        progress: [0.0, 0.0],
                        active: 0,
                    };
                    p.replanned = 0;
                    p.contact_reported = false;
                    events.push(Event::OrderReceived { side: c.side, to: Addressee::Platoon(*platoon) });
                }
            }
            Command::SetFormation { platoon, formation } => {
                let f = *formation;
                if let Some(p) = self.platoon_mut(c.side, *platoon) {
                    p.formation = f;
                    p.replanned = 0;
                }
            }
            Command::OccupyPosition { platoon, centre, radius_m, facing, kind } => {
                let o = platoon::PlatoonOrder::Occupy {
                    centre: *centre,
                    radius: *radius_m,
                    facing: *facing,
                    kind: *kind,
                    assigned: false,
                    reported: false,
                };
                if let Some(p) = self.platoon_mut(c.side, *platoon) {
                    p.order = o;
                    events.push(Event::OrderReceived { side: c.side, to: Addressee::Platoon(*platoon) });
                }
            }
        }
    }

    fn update_pictures(&mut self) {
        for side in self.sides.iter_mut() {
            let views: Vec<_> =
                self.platoons.iter().filter(|p| p.side == side.picture.side).map(|p| p.view(&self.world)).collect();
            side.rebuild_own(&self.world, &self.vehicles, &views, self.tick);
        }
        for (dst, src) in self.pictures.iter_mut().zip(&self.sides) {
            dst.clone_from(&src.picture);
        }
    }

    /// Hash of the full simulation state (golden replay tests, desync detection).
    pub fn state_hash(&self) -> u64 {
        let mut h = StableHasher::new();
        h.u64(self.tick);
        self.world.hash_into(&mut h);
        for s in &self.sides {
            h.u64(s.picture.contacts.len() as u64);
            for c in &s.picture.contacts {
                h.u32(c.id.0);
                h.f64(c.pos[0]);
                h.f64(c.pos[1]);
            }
        }
        for r in [&self.rngs.mobility, &self.rngs.sensors, &self.rngs.gunnery, &self.rngs.ai, &self.rngs.comms] {
            h.u64(r.get_word_pos() as u64);
        }
        h.finish()
    }
}
