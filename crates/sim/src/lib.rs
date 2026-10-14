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
pub mod control;
pub mod fires;
pub mod gunnery;
pub mod mobility;
pub mod picture;
pub mod plan;
pub mod platoon;
pub mod platoon_ai;
pub mod sensors;
pub mod smoke;
pub mod vehicle;
pub mod victory;
pub mod weather;
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
pub use command::{
    Addressee, Command, DenyReason, EndReason, Event, FireKind, FireReport, KillKind, PlatoonReport, PositionKind,
    SmokeMode, TimedCommand,
};
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
    pub fires: ChaCha8Rng,
}

impl Rngs {
    fn new(seed: u64) -> Self {
        let s =
            |k: u64| ChaCha8Rng::seed_from_u64(terrain::noise::hash_u64(seed ^ k.wrapping_mul(0x9e37_79b9_7f4a_7c15)));
        Self { mobility: s(1), sensors: s(2), gunnery: s(3), ai: s(4), comms: s(5), fires: s(6) }
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
    /// Per vehicle definition: index into `ammo` for each entry of its loadout.
    loadouts: Vec<Vec<usize>>,
    round_labels: Vec<Vec<&'static str>>,
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
    commanders: Vec<Option<ai::commander::Commander>>,
    ai_enabled: Vec<bool>,
    victory: victory::VictoryState,
    smoke: smoke::SmokeField,
    weather: weather::Weather,
    batteries: Vec<fires::Battery>,
    /// Recent shell bursts (pruned after `IMPACT_KEEP_S`).
    impacts: Vec<fires::Impact>,
    /// Per side: the equipment year (scenario date or the side's tech year).
    years: Vec<i32>,
}

/// Commander AI decision interval: 0.2 Hz (DESIGN.md §4.1).
pub const COMMANDER_EVERY: u64 = 100;
/// Smoke clouds are in a side's picture when within this distance of one of its vehicles (m).
const SMOKE_VIEW_M: f64 = 3000.0;
/// Shell bursts stay in the pictures and the react-to-indirect-fire drill this long (s).
const IMPACT_KEEP_S: f64 = 30.0;
/// Bursts in a side's picture when within this distance of one of its vehicles (m).
const IMPACT_VIEW_M: f64 = 1500.0;

impl Sim {
    pub fn new(scenario: &ScenarioDef, data: Arc<DataSet>, map: Arc<BakedMap>) -> Result<Self, String> {
        let mut world = World::default();
        let mut vehicles: Vec<VehicleDef> = Vec::new();
        let mut ammo: Vec<AmmoDef> = Vec::new();
        let mut loadouts: Vec<Vec<usize>> = Vec::new();
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
                        let mut lo = Vec::new();
                        for l in &def.armament.load {
                            let round = data.ammo(&l.ammo).ok_or_else(|| format!("{}: no ammunition", def.id))?;
                            lo.push(match ammo.iter().position(|a| a.id == round.id) {
                                Some(i) => i,
                                None => {
                                    ammo.push(round.clone());
                                    ammo.len() - 1
                                }
                            });
                        }
                        loadouts.push(lo);
                        vehicles.len() - 1
                    }
                };
                // Rounds not yet in service at the side's date are not carried (unless none are).
                let in_service = |id: &str| data.ammo(id).is_some_and(|a| a.in_service.contains(year));
                let any = def.armament.load.iter().any(|l| in_service(&l.ammo));
                let stock: Vec<u32> =
                    def.armament.load.iter().map(|l| if !any || in_service(&l.ammo) { l.count } else { 0 }).collect();
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
                    stock,
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
                                held: None,
                                speed_limit: None,
                                quiet_since: 0,
                                last_strength: 0,
                                last_indirect: None,
                                withdraw: false,
                                plan: None,
                                finished: Vec::new(),
                                contact_reaction: Default::default(),
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
        let commanders: Vec<Option<ai::commander::Commander>> = scenario
            .sides
            .iter()
            .enumerate()
            .map(|(si, side)| {
                side.ai.as_ref().map(|plan| {
                    let tasks = plan
                        .tasks
                        .iter()
                        .map(|t| ai::commander::TaskState {
                            platoons: t
                                .platoons
                                .iter()
                                .filter_map(|cs| {
                                    platoons.iter().find(|p| p.side.0 as usize == si && &p.callsign == cs).map(|p| p.id)
                                })
                                .collect(),
                            mission: t.mission.clone(),
                            start_s: t.start_s,
                            phase: ai::commander::Phase::Waiting,
                            broken: Vec::new(),
                        })
                        .collect();
                    ai::commander::Commander::new(tasks)
                })
            })
            .collect();
        let ai_enabled = commanders.iter().map(|c| c.is_some()).collect();
        let smoke =
            smoke::SmokeField::new(&data.smoke, scenario.weather.wind_from_deg, scenario.weather.wind_speed_mps);
        let weather = weather::Weather::new(scenario, &data.weather)?;
        let years: Vec<i32> = scenario.sides.iter().map(|s| s.tech_year.unwrap_or(scenario.date.0)).collect();
        let mut batteries = Vec::new();
        for (si, side) in scenario.sides.iter().enumerate() {
            for a in &side.fire_support {
                let di = data
                    .fires
                    .batteries
                    .iter()
                    .position(|b| b.id == a.battery)
                    .ok_or_else(|| format!("{}: unknown battery '{}'", a.callsign, a.battery))?;
                let d = &data.fires.batteries[di];
                if !d.in_service.contains(years[si]) {
                    return Err(format!("{}: {} is not in service in {}", a.callsign, d.name, years[si]));
                }
                let sk = data
                    .smoke
                    .kind(&d.smoke_kind)
                    .ok_or_else(|| format!("{}: unknown smoke kind '{}'", d.id, d.smoke_kind))?;
                let mut b = fires::Battery::new(SideId(si as u8), a.callsign.clone(), di, d, a.position, sk);
                if let Some(n) = a.he_rounds {
                    b.he = n;
                }
                if let Some(n) = a.smoke_rounds {
                    b.smoke = n;
                }
                batteries.push(b);
            }
        }
        let mut sim = Self {
            tick: 0,
            map,
            data,
            vehicles,
            round_labels: loadouts.iter().map(|lo| lo.iter().map(|&i| ammo[i].kind.label()).collect()).collect(),
            loadouts,
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
            commanders,
            ai_enabled,
            victory: victory::VictoryState::default(),
            smoke,
            weather,
            batteries,
            impacts: Vec::new(),
            years,
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

    /// The doctrine pack a side uses (its own data; the UI reads speed presets from it).
    pub fn doctrine(&self, side: SideId) -> Option<&sim_data::DoctrinePack> {
        self.sides.get(side.0 as usize).map(|s| &s.doctrine)
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

    /// Turn a side's commander AI on or off (the player's side normally runs without it).
    pub fn set_ai_enabled(&mut self, side: SideId, on: bool) {
        if let Some(v) = self.ai_enabled.get_mut(side.0 as usize) {
            *v = on && self.commanders[side.0 as usize].is_some();
        }
    }

    /// Commander phase per task, for display/debugging (AI side only).
    pub fn commander_phases(&self, side: SideId) -> Vec<ai::commander::Phase> {
        self.commanders
            .get(side.0 as usize)
            .and_then(|c| c.as_ref())
            .map(|c| c.tasks.iter().map(|t| t.phase).collect())
            .unwrap_or_default()
    }

    /// Queue an order over the radio net: it takes effect after the side's doctrinal order
    /// delay (DESIGN.md §8.2). Logged for replay.
    pub fn submit(&mut self, side: SideId, cmd: Command) {
        self.incoming.push(TimedCommand { tick: self.tick, side, cmd, immediate: false, generated: false });
    }

    /// Pre-planned order (code word, trigger, planning phase): no radio delay.
    pub fn submit_immediate(&mut self, side: SideId, cmd: Command) {
        self.incoming.push(TimedCommand { tick: self.tick, side, cmd, immediate: true, generated: false });
    }

    pub fn command_log(&self) -> &[TimedCommand] {
        &self.log
    }

    /// Commands given from outside the simulation (player/scripts): the replay input.
    pub fn player_log(&self) -> Vec<TimedCommand> {
        self.log.iter().filter(|c| !c.generated).cloned().collect()
    }

    /// Apply submitted control measures (graphics, triggers) now without advancing time, so
    /// they show during a planning phase. Orders stay queued for the next step. Control
    /// measures draw no random numbers, so a replay that applies them in `step` at the same
    /// tick reaches the same state.
    pub fn settle(&mut self) {
        let (control, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.incoming).into_iter().partition(|c| {
            matches!(
                c.cmd,
                Command::AddGraphic { .. }
                    | Command::RemoveGraphic { .. }
                    | Command::AddTrigger { .. }
                    | Command::CodeWord { .. }
            )
        });
        self.incoming = rest;
        let mut events = Vec::new();
        for c in control {
            self.log.push(c.clone());
            self.apply(&c, &mut events);
        }
        for side in self.sides.iter_mut() {
            side.picture.triggers = control::views(&side.triggers);
        }
        self.update_pictures();
    }

    /// Re-inject a logged command (replays). Call before `step` at the command's tick.
    pub fn inject(&mut self, c: TimedCommand) {
        self.incoming.push(c);
    }

    pub fn perceived(&self, side: SideId) -> &PerceivedWorld {
        &self.pictures[side.0 as usize]
    }

    /// Ground truth. Only for AAR, tests and the client's `debug-truth` feature.
    pub fn truth(&self) -> &World {
        &self.world
    }

    /// Ground truth, writable: tests only (scenario set-up such as knocking out a vehicle).
    #[doc(hidden)]
    pub fn truth_mut(&mut self) -> &mut World {
        &mut self.world
    }

    pub fn step(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        let tick = self.tick;

        // 1. Orders: draw radio delays in submission order, then deliver due orders.
        for c in std::mem::take(&mut self.incoming) {
            let control = matches!(
                c.cmd,
                Command::AddGraphic { .. }
                    | Command::RemoveGraphic { .. }
                    | Command::AddTrigger { .. }
                    | Command::CodeWord { .. }
            );
            let delay = if c.immediate || control {
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

        // 1b. Triggers (1 Hz): release contingency orders as code words.
        if tick.is_multiple_of(PLATOON_AI_EVERY) {
            for si in 0..self.sides.len() {
                let pic = &self.pictures[si];
                let mut fire = Vec::new();
                for t in self.sides[si].triggers.iter_mut().filter(|t| !t.fired) {
                    if control::met(pic, t) {
                        t.fired = true;
                        fire.push((t.id, t.then.clone()));
                    }
                }
                for (id, cmd) in fire {
                    let side = SideId(si as u8);
                    events.push(Event::TriggerFired { side, id });
                    self.incoming.push(TimedCommand { tick, side, cmd, immediate: true, generated: true });
                }
                let views = control::views(&self.sides[si].triggers);
                self.sides[si].picture.triggers = views;
            }
        }

        // 2a. Commander AI (orders go over the radio net like the player's).
        if tick.is_multiple_of(COMMANDER_EVERY) {
            for si in 0..self.commanders.len() {
                if !self.ai_enabled[si] {
                    continue;
                }
                let Some(cmdr) = self.commanders[si].as_mut() else { continue };
                let orders = cmdr.step(&self.pictures[si], &self.sides[si].doctrine, &self.scenario.objectives);
                let side = SideId(si as u8);
                for o in orders {
                    let cmd = match o {
                        ai::commander::Order::Move { platoon, destination, formation, technique } => {
                            Command::PlatoonMove {
                                platoon,
                                destination,
                                via: Vec::new(),
                                formation: Some(formation),
                                technique,
                                speed_limit_mps: None,
                            }
                        }
                        ai::commander::Order::Occupy { platoon, centre, radius, facing } => Command::OccupyPosition {
                            platoon,
                            centre,
                            radius_m: radius,
                            facing,
                            kind: PositionKind::HullDown,
                        },
                        ai::commander::Order::Halt { platoon } => Command::Halt { to: Addressee::Platoon(platoon) },
                    };
                    self.incoming.push(TimedCommand { tick, side, cmd, immediate: tick == 0, generated: true });
                }
            }
        }

        // 2. Platoon AI.
        if tick.is_multiple_of(PLATOON_AI_EVERY) {
            let mut sim_cmds = Vec::new();
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
                impacts: &self.impacts,
                fires: &self.data.fires.suppression,
                sim_cmds: &mut sim_cmds,
            };
            platoon_ai::step(&mut self.platoons, &mut ctx);
            for (side, cmd) in sim_cmds {
                self.apply(&TimedCommand { tick, side, cmd, immediate: true, generated: true }, &mut events);
            }
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
                &self.weather.at(self.start_time_s as f64 + tick as f64 * DT, &self.data.detection),
                self.data.fires.suppression.sensor_penalty.get(),
                tick,
                &mut self.rngs.sensors,
                &mut self.los_cache,
                &self.smoke,
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
            &self.loadouts,
            &self.ammo,
            &self.data.penetration,
            &self.data.damage,
            &self.data.smoke,
            &engage,
            self.data.fires.suppression.slow_gain.get(),
            tick,
            &mut self.rngs.gunnery,
        );
        for f in fired {
            let p = &f.projectile;
            let r = dist(p.from, p.to);
            let hit = matches!(&p.effect, gunnery::Effect::Shot { outcome, .. } if outcome.hit);
            events.push(Event::Fired { unit: self.world.id[f.shooter], range_m: r, from: p.from, to: p.to, hit });
            let shooter_side = self.world.side[f.shooter];
            for side in self.sides.iter_mut() {
                let own = side.picture.side == shooter_side;
                // The other side sees the shot if it already has the shooter as a contact.
                let known = own || side.picture.contacts.iter().any(|c| !c.destroyed && dist(c.pos, p.from) < 300.0);
                if known {
                    side.add_shot(ShotView { from: p.from, to: p.to, time_s: tick as f64 * DT, hit, own });
                }
            }
            self.projectiles.push(f.projectile);
        }

        // 7. Impacts.
        let (hits, flying): (Vec<_>, Vec<_>) =
            std::mem::take(&mut self.projectiles).into_iter().partition(|p| p.impact_tick <= tick);
        self.projectiles = flying;
        for p in hits {
            let (target, o) = match &p.effect {
                gunnery::Effect::Smoke { kind, clouds } => {
                    for i in 0..*clouds {
                        let a = i as f64 * 2.4;
                        let off = if *clouds > 1 { 8.0 } else { 0.0 };
                        self.smoke.add(*kind, [p.to[0] + libm::sin(a) * off, p.to[1] + libm::cos(a) * off]);
                    }
                    continue;
                }
                gunnery::Effect::Shot { target, outcome } => (*target, outcome),
            };
            if self.world.alive(target) {
                let sup = &self.data.fires.suppression;
                let add = if !o.hit {
                    sup.near_miss.get()
                } else if !o.penetrated {
                    sup.glancing_hit.get()
                } else {
                    0.0
                };
                let s = &mut self.world.suppression[target];
                *s = (*s + add).min(1.0);
                let bearing = ai::drive::bearing(self.world.transform[target].pos, p.from);
                self.world.smoke[target].last_targeted = Some((tick, bearing));
            }
            let kill = if o.hit && self.world.alive(target) {
                if o.era_fired {
                    self.world.era[target] = self.world.era[target].saturating_sub(1);
                }
                gunnery::apply_damage(&mut self.world, target, o.kill, o.sights_damaged)
            } else {
                None
            };
            events.push(Event::ShotResult { unit: self.world.id[p.shooter], hit: o.hit, kill });
            if o.hit {
                events.push(Event::Hit { unit: self.world.id[target], penetrated: o.penetrated, kill });
            }
            if kill.is_some() && self.world.condition[target] == Condition::Destroyed {
                let s = self.world.side[p.shooter].0 as usize;
                self.sides[s].mark_destroyed(self.world.transform[target].pos);
            }
        }

        // 7a. Fire support: volleys, rounds in flight, bursts; suppression decays.
        {
            let mut fctx = fires::Ctx {
                model: &self.data.fires,
                world: &mut self.world,
                vehicles: &self.vehicles,
                smoke: &mut self.smoke,
                tick,
                rng: &mut self.rngs.fires,
                events: &mut events,
                impacts: &mut self.impacts,
            };
            for (side, pos) in fires::step(&mut self.batteries, &mut fctx) {
                self.sides[side.0 as usize].mark_destroyed(pos);
            }
            let decay = self.data.fires.suppression.decay_per_s.get() * DT;
            for s in self.world.suppression.iter_mut() {
                *s = (*s - decay).max(0.0);
            }
            self.impacts.retain(|i| (tick - i.tick) as f64 * DT <= IMPACT_KEEP_S);
        }

        // 7b. Smoke: self-screening, engine smoke, then the clouds themselves.
        let doctrines: Vec<&sim_data::DoctrinePack> = self.sides.iter().map(|s| &s.doctrine).collect();
        smoke::units(&mut self.smoke, &mut self.world, &self.vehicles, &doctrines, &self.years, tick, &mut events);
        self.smoke.step(DT);

        // 8. Mobility.
        mobility::step(&mut self.world, &self.vehicles, &self.map, &self.data.terrain, &mut events);

        // 9. Victory conditions (1 Hz).
        if tick.is_multiple_of(PLATOON_AI_EVERY)
            && !self.ended
            && let Some(v) = self.scenario.victory.clone()
            && let Some((winner, reason)) = self.victory.check(&self.world, &self.scenario, &v, tick as f64 * DT)
        {
            self.ended = true;
            events.push(Event::ScenarioEnd { winner, reason });
        }

        self.tick += 1;
        self.update_pictures();
        events
    }

    fn platoon_mut(&mut self, side: SideId, id: PlatoonId) -> Option<&mut platoon::Platoon> {
        self.platoons.iter_mut().find(|p| p.id == id && p.side == side)
    }

    /// Compass bearing from vehicle `k` to the nearest live contact of its side's picture, or its
    /// turret heading when none is known.
    fn nearest_threat_bearing(&self, side: SideId, k: usize) -> f64 {
        let pos = self.world.transform[k].pos;
        self.pictures[side.0 as usize]
            .live_contacts()
            .min_by(|a, b| dist(a.pos, pos).total_cmp(&dist(b.pos, pos)))
            .map(|c| ai::drive::bearing(pos, c.pos))
            .unwrap_or_else(|| self.world.gun[k].as_ref().map_or(self.world.transform[k].heading, |g| g.turret))
    }

    /// Break contact (ATP 3-20.15 App. F): screen with smoke and withdraw directly away from the
    /// nearest known threat by the doctrine distance, in column.
    fn break_contact(&mut self, side: SideId, id: PlatoonId, events: &mut Vec<Event>) {
        let Some(pi) = self.platoons.iter().position(|p| p.id == id && p.side == side) else { return };
        let alive = self.platoons[pi].alive_members(&self.world);
        let Some(&lead) = alive.first() else { return };
        let n = alive.len() as f64;
        let centre = [
            alive.iter().map(|&k| self.world.transform[k].pos[0]).sum::<f64>() / n,
            alive.iter().map(|&k| self.world.transform[k].pos[1]).sum::<f64>() / n,
        ];
        let pic = &self.pictures[side.0 as usize];
        let threat = pic
            .live_contacts()
            .min_by(|a, b| dist(a.pos, centre).total_cmp(&dist(b.pos, centre)))
            .map(|c| c.pos)
            .unwrap_or_else(|| {
                let h = self.platoons[pi].heading;
                [centre[0] + libm::sin(h) * 1000.0, centre[1] + libm::cos(h) * 1000.0]
            });
        let away = ai::drive::bearing(threat, centre);
        let distance = self.sides[side.0 as usize].doctrine.break_contact_distance_m.get();
        let Some(vi) = self.world.vehicle[lead] else { return };
        let Some(layer) = self.map.mobility(&self.vehicles[vi].id) else { return };
        let cell = self.map.manifest.cell_size_m;
        let (w, h) = (self.map.manifest.width_m(), self.map.manifest.height_m());
        // Straight away if a route exists, else fan out to either side.
        let mut dest = None;
        for step in 0..9 {
            let off = (step as f64 / 2.0).ceil() * 20f64.to_radians() * if step % 2 == 0 { 1.0 } else { -1.0 };
            let b = away + off;
            let d = [centre[0] + libm::sin(b) * distance, centre[1] + libm::cos(b) * distance];
            let d = [d[0].clamp(cell, w - cell), d[1].clamp(cell, h - cell)];
            if dist(d, centre) > distance * 0.5 && self.pathfinder.find(&layer.speed, cell, centre, d).is_some() {
                dest = Some(d);
                break;
            }
        }
        let year = self.years[side.0 as usize];
        for &k in &alive {
            if smoke::launch(
                &mut self.smoke,
                &mut self.world,
                &self.vehicles,
                k,
                away + std::f64::consts::PI,
                year,
                self.tick,
            ) {
                events.push(Event::SmokeLaunched { unit: self.world.id[k], auto: false });
            }
        }
        let Some(dest) = dest else {
            events.push(Event::PlatoonReport { side, platoon: id, report: PlatoonReport::NoRoute });
            return;
        };
        let p = &mut self.platoons[pi];
        p.formation = sim_data::Formation::Column;
        p.technique = sim_data::Technique::Traveling;
        p.speed_limit = None;
        p.order = platoon::PlatoonOrder::new_move(dest, Vec::new());
        p.held = None;
        p.resume = None;
        p.replanned = 0;
        p.contact_reported = false;
        p.withdraw = true;
        events.push(Event::OrderReceived { side, to: Addressee::Platoon(id) });
        events.push(Event::PlatoonReport { side, platoon: id, report: PlatoonReport::BreakingContact { threat } });
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
            Command::Move { unit, destination, via, speed_limit_mps } => {
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
                    p.plan = None;
                }
                let layer = self.map.mobility(&self.vehicles[vi].id).expect("checked at spawn");
                let pos = self.world.transform[k].pos;
                let cell = self.map.manifest.cell_size_m;
                match self.pathfinder.find_via(&layer.speed, cell, pos, via, *destination) {
                    Some((route, _)) => {
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
                    p.halt();
                    if let Some(st) = p.plan.as_mut() {
                        st.paused = true;
                    }
                    events.push(Event::OrderReceived { side: c.side, to: *to });
                }
            }
            Command::Resume { to } => {
                let resumed = match to {
                    Addressee::Platoon(p) => self.platoon_mut(c.side, *p).is_some_and(|p| {
                        let was_paused = p.plan.as_mut().is_some_and(|st| std::mem::take(&mut st.paused));
                        match p.held.take() {
                            Some(o) => {
                                p.order = o;
                                p.replanned = 0;
                                p.contact_reported = false;
                                true
                            }
                            None => was_paused,
                        }
                    }),
                    Addressee::Unit(_) => self.members(c.side, *to).into_iter().any(|k| self.world.resume(k)),
                };
                if resumed {
                    events.push(Event::OrderReceived { side: c.side, to: *to });
                }
            }
            Command::SetSpeed { to, speed_limit_mps } => {
                match to {
                    Addressee::Platoon(p) => {
                        if let Some(p) = self.platoon_mut(c.side, *p) {
                            p.speed_limit = *speed_limit_mps;
                        }
                    }
                    Addressee::Unit(_) => {
                        for k in self.members(c.side, *to) {
                            self.world.set_speed_limit(k, *speed_limit_mps);
                        }
                    }
                }
                events.push(Event::OrderReceived { side: c.side, to: *to });
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
            Command::PlatoonMove { platoon, destination, via, formation, technique, speed_limit_mps } => {
                let (dest, f, t) = (*destination, *formation, *technique);
                if let Some(p) = self.platoon_mut(c.side, *platoon) {
                    if let Some(f) = f {
                        p.formation = f;
                    }
                    p.technique = t;
                    p.speed_limit = *speed_limit_mps;
                    p.order = platoon::PlatoonOrder::new_move(dest, via.clone());
                    p.plan = None;
                    p.held = None;
                    p.withdraw = false;
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
            Command::Smoke { to, mode } => {
                for k in self.members(c.side, *to) {
                    let year = self.years[c.side.0 as usize];
                    match mode {
                        SmokeMode::Screen => {
                            let bearing = self.nearest_threat_bearing(c.side, k);
                            if smoke::launch(
                                &mut self.smoke,
                                &mut self.world,
                                &self.vehicles,
                                k,
                                bearing,
                                year,
                                self.tick,
                            ) {
                                events.push(Event::SmokeLaunched { unit: self.world.id[k], auto: false });
                            }
                        }
                        SmokeMode::EngineOn | SmokeMode::EngineOff => {
                            let has = self.world.vehicle[k].is_some_and(|vi| self.vehicles[vi].engine_smoke.is_some());
                            if has {
                                self.world.smoke[k].engine_on = *mode == SmokeMode::EngineOn;
                            }
                        }
                    }
                }
                events.push(Event::OrderReceived { side: c.side, to: *to });
            }
            Command::BreakContact { platoon } => {
                if let Some(p) = self.platoon_mut(c.side, *platoon) {
                    p.plan = None;
                }
                self.break_contact(c.side, *platoon, events)
            }
            Command::SetPlan { platoon, plan, start } => {
                let Some(p) = self.platoon_mut(c.side, *platoon) else { return };
                p.finished.clear();
                if plan.steps.is_empty() {
                    p.plan = None;
                } else if let Some(idx) = plan.index_of(*start) {
                    p.plan = Some(match p.plan.take() {
                        Some(old) => old.edited(plan.clone(), idx),
                        None => {
                            p.held = None;
                            plan::PlanState::new(plan.clone(), idx)
                        }
                    });
                } else {
                    return;
                }
                events.push(Event::OrderReceived { side: c.side, to: Addressee::Platoon(*platoon) });
            }
            Command::SelectAmmo { to, kind } => {
                for k in self.members(c.side, *to) {
                    if let Some(g) = self.world.gun[k].as_mut() {
                        g.selected = *kind;
                    }
                }
                events.push(Event::OrderReceived { side: c.side, to: *to });
            }
            Command::FireSmoke { to, at, rounds } => {
                let members: Vec<usize> = self
                    .members(c.side, *to)
                    .into_iter()
                    .filter(|&k| self.world.alive(k) && self.world.condition[k] != Condition::FirepowerKill)
                    .collect();
                let n = members.len() as f64;
                let from = members.first().map_or(*at, |&k| self.world.transform[k].pos);
                let b = ai::drive::bearing(from, *at);
                for (i, &k) in members.iter().enumerate() {
                    // Spread the platoon's rounds across a screen, 40 m apart, square to the line of fire.
                    let off = (i as f64 - (n - 1.0) / 2.0) * 40.0;
                    let aim = [at[0] + libm::cos(b) * off, at[1] - libm::sin(b) * off];
                    if let Some(g) = self.world.gun[k].as_mut() {
                        g.mission = Some(gunnery::SmokeMission { at: aim, rounds_left: (*rounds).max(1) });
                    }
                }
                events.push(Event::OrderReceived { side: c.side, to: *to });
            }
            Command::CallForFire { battery, at, kind, volleys } => {
                let Some(b) = self.batteries.get_mut(*battery as usize).filter(|b| b.side == c.side) else { return };
                let d = &self.data.fires.batteries[b.def];
                let report = match b.call(d, *at, *kind, *volleys, self.tick) {
                    Ok(eta_s) => FireReport::Accepted { eta_s },
                    Err(why) => FireReport::Denied(why),
                };
                events.push(Event::FireSupport { side: c.side, battery: *battery, at: *at, report });
            }
            Command::CeaseFire { battery } => {
                let Some(b) = self.batteries.get_mut(*battery as usize).filter(|b| b.side == c.side) else { return };
                let at = b.mission.as_ref().map_or(b.pos, |m| m.at);
                if fires::cease(b) {
                    events.push(Event::FireSupport { side: c.side, battery: *battery, at, report: FireReport::Ceased });
                }
            }
            Command::SetContactReaction { platoon, reaction } => {
                if let Some(p) = self.platoon_mut(c.side, *platoon) {
                    p.contact_reaction = *reaction;
                    events.push(Event::OrderReceived { side: c.side, to: Addressee::Platoon(*platoon) });
                }
            }
            Command::CodeWord { word } => {
                let s = &mut self.sides[c.side.0 as usize];
                s.picture.code_words.retain(|w| w.word != *word);
                s.picture.code_words.push(ai::picture::CodeWordView { word: *word, tick: self.tick });
                events.push(Event::CodeWordSent { side: c.side, word: *word });
            }
            Command::AddGraphic { graphic } => {
                let s = &mut self.sides[c.side.0 as usize];
                s.next_graphic += 1;
                let mut g = graphic.clone();
                g.id = s.next_graphic;
                s.picture.graphics.push(g);
            }
            Command::RemoveGraphic { id } => {
                self.sides[c.side.0 as usize].picture.graphics.retain(|g| g.id != *id);
            }
            Command::AddTrigger { condition, then } => {
                let s = &mut self.sides[c.side.0 as usize];
                let sign = control::reference_sign(&s.picture, condition);
                let id = s.triggers.len() as u32 + 1;
                s.triggers.push(control::Trigger {
                    id,
                    condition: condition.clone(),
                    then: (**then).clone(),
                    fired: false,
                    reference_sign: sign,
                });
                s.picture.triggers = control::views(&s.triggers);
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
                    p.plan = None;
                    p.held = None;
                    p.withdraw = false;
                    events.push(Event::OrderReceived { side: c.side, to: Addressee::Platoon(*platoon) });
                }
            }
        }
    }

    fn update_pictures(&mut self) {
        let weather = self.weather.view(&self.weather.at(self.clock_s(), &self.data.detection));
        for side in self.sides.iter_mut() {
            let views: Vec<_> =
                self.platoons.iter().filter(|p| p.side == side.picture.side).map(|p| p.view(&self.world)).collect();
            side.rebuild_own(&self.world, &self.vehicles, &self.round_labels, &views, self.tick);
        }
        for (dst, src) in self.pictures.iter_mut().zip(&self.sides) {
            dst.clone_from(&src.picture);
            dst.weather = weather.clone();
            let own =
                |p: Vec2| dst.own.iter().any(|u| u.condition != Condition::Destroyed && dist(u.pos, p) < IMPACT_VIEW_M);
            dst.impacts = fires::impact_views(&self.impacts, own, self.tick, IMPACT_KEEP_S);
            dst.fire_support = self
                .batteries
                .iter()
                .enumerate()
                .filter(|(_, b)| b.side == dst.side)
                .map(|(i, b)| b.view(i as u32, &self.data.fires.batteries[b.def], self.tick))
                .collect();
            if !self.smoke.is_empty() {
                let near = |p: Vec2| {
                    dst.own.iter().any(|u| u.condition != Condition::Destroyed && dist(u.pos, p) < SMOKE_VIEW_M)
                };
                dst.smoke = self.smoke.views().into_iter().filter(|v| near(v.pos)).collect();
            }
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
        for s in &self.sides {
            h.u64(s.triggers.iter().filter(|t| t.fired).count() as u64);
        }
        self.smoke.hash_into(&mut h);
        for p in &self.platoons {
            if let Some(st) = &p.plan {
                st.hash_into(p.id, &mut h);
            }
            if p.contact_reaction != ai::picture::ContactReaction::Drill {
                h.u32(p.id.0);
                h.u8(p.contact_reaction as u8);
            }
        }
        for b in &self.batteries {
            b.hash_into(&mut h);
        }
        for r in [
            &self.rngs.mobility,
            &self.rngs.sensors,
            &self.rngs.gunnery,
            &self.rngs.ai,
            &self.rngs.comms,
            &self.rngs.fires,
        ] {
            h.u64(r.get_word_pos() as u64);
        }
        h.finish()
    }
}

/// A recorded game: scenario id plus the external command log. Re-simulating it reproduces the
/// game exactly (determinism), which is what the after-action review and replays rely on.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Replay {
    pub scenario: String,
    pub commands: Vec<TimedCommand>,
    pub final_tick: u64,
    pub final_hash: u64,
    /// Sides played by a human (commander AI switched off).
    #[serde(default)]
    pub player_sides: Vec<SideId>,
}

impl Replay {
    pub fn record(sim: &Sim) -> Self {
        let player_sides = (0..sim.commanders.len())
            .filter(|&si| sim.commanders[si].is_some() && !sim.ai_enabled[si])
            .map(|si| SideId(si as u8))
            .collect();
        Self {
            scenario: sim.scenario.id.clone(),
            commands: sim.player_log(),
            final_tick: sim.tick,
            final_hash: sim.state_hash(),
            player_sides,
        }
    }

    pub fn to_ron(&self) -> String {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default()).unwrap_or_default()
    }

    pub fn from_ron(s: &str) -> Result<Self, String> {
        ron::from_str(s).map_err(|e| e.to_string())
    }

    /// Re-simulate on a fresh `Sim` to the recorded tick, calling `each` after every step.
    pub fn play(&self, sim: &mut Sim, mut each: impl FnMut(&Sim, &[Event])) {
        for &s in &self.player_sides {
            sim.set_ai_enabled(s, false);
        }
        let mut cmds = self.commands.clone();
        cmds.sort_by_key(|c| c.tick);
        let mut next = 0;
        while sim.tick() < self.final_tick {
            while next < cmds.len() && cmds[next].tick <= sim.tick() {
                sim.inject(cmds[next].clone());
                next += 1;
            }
            let ev = sim.step();
            each(sim, &ev);
        }
    }
}
