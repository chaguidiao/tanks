//! The simulation core: deterministic, fixed 20 Hz tick, no graphics (CLAUDE.md rules 1–2).
//!
//! API boundary (DESIGN.md §4.2):
//! ```text
//! sim.submit(side, cmd)    orders in (timestamped, logged for replay)
//! sim.step() -> events     advance one tick
//! sim.perceived(side)      what a side knows (players and AI read only this)
//! sim.truth()              ground truth: AAR, tests, debug-truth only
//! ```

pub mod command;
pub mod mobility;
pub mod perceived;
pub mod world;

use std::sync::Arc;

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;
use sim_data::{DataSet, ScenarioDef, VehicleDef};
use terrain::BakedMap;
use terrain::hash::StableHasher;
use terrain::path::Pathfinder;

pub use command::{Command, Event, TimedCommand};
pub use perceived::{OwnUnit, PerceivedWorld, Task};
pub use world::{EntityId, SideId, World};

pub type Vec2 = [f64; 2];

/// Simulation rate (DESIGN.md §4.1). Time compression runs more ticks, never a larger step.
pub const TICK_HZ: u32 = 20;
pub const DT: f64 = 1.0 / TICK_HZ as f64;
/// Vehicle AI decision rate (DESIGN.md §4.1): every 4th tick = 5 Hz.
pub const VEHICLE_AI_EVERY: u64 = 4;

/// One seeded ChaCha stream per subsystem (CLAUDE.md rule 2). Unused streams are reserved so
/// adding a consumer later does not shift the others.
#[derive(Debug, Clone)]
pub struct Rngs {
    pub mobility: ChaCha8Rng,
    pub sensors: ChaCha8Rng,
    pub gunnery: ChaCha8Rng,
    pub ai: ChaCha8Rng,
}

impl Rngs {
    fn new(seed: u64) -> Self {
        let s =
            |k: u64| ChaCha8Rng::seed_from_u64(terrain::noise::hash_u64(seed ^ k.wrapping_mul(0x9e37_79b9_7f4a_7c15)));
        Self { mobility: s(1), sensors: s(2), gunnery: s(3), ai: s(4) }
    }
}

pub struct Sim {
    tick: u64,
    map: Arc<BakedMap>,
    data: Arc<DataSet>,
    vehicles: Vec<VehicleDef>,
    world: World,
    pending: Vec<TimedCommand>,
    log: Vec<TimedCommand>,
    perceived: Vec<PerceivedWorld>,
    side_names: Vec<String>,
    pathfinder: Pathfinder,
    rngs: Rngs,
    start_time_s: u32,
}

impl Sim {
    pub fn new(scenario: &ScenarioDef, data: Arc<DataSet>, map: Arc<BakedMap>) -> Result<Self, String> {
        let mut world = World::default();
        let mut vehicles: Vec<VehicleDef> = Vec::new();
        for (si, side) in scenario.sides.iter().enumerate() {
            let year = side.tech_year.unwrap_or(scenario.date.0);
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
                        vehicles.len() - 1
                    }
                };
                let heading = u.heading_deg.to_radians();
                world.spawn_vehicle(SideId(si as u8), &u.callsign, vi, &vehicles[vi], u.position, heading, &map);
            }
        }
        let n_sides = scenario.sides.len();
        let mut sim = Self {
            tick: 0,
            map,
            data,
            vehicles,
            world,
            pending: Vec::new(),
            log: Vec::new(),
            perceived: (0..n_sides).map(|s| PerceivedWorld::empty(SideId(s as u8))).collect(),
            side_names: scenario.sides.iter().map(|s| s.name.clone()).collect(),
            pathfinder: Pathfinder::new(),
            rngs: Rngs::new(scenario.seed),
            start_time_s: scenario.start_time_s,
        };
        sim.update_perceived();
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

    pub fn side_names(&self) -> &[String] {
        &self.side_names
    }

    /// Queue an order. It takes effect at the start of the next tick and is logged for replay.
    pub fn submit(&mut self, side: SideId, cmd: Command) {
        self.pending.push(TimedCommand { tick: self.tick, side, cmd });
    }

    pub fn command_log(&self) -> &[TimedCommand] {
        &self.log
    }

    pub fn perceived(&self, side: SideId) -> &PerceivedWorld {
        &self.perceived[side.0 as usize]
    }

    /// Ground truth. Only for AAR, tests and the client's `debug-truth` feature.
    pub fn truth(&self) -> &World {
        &self.world
    }

    pub fn step(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        let cmds = std::mem::take(&mut self.pending);
        for c in cmds {
            self.apply(&c, &mut events);
            self.log.push(c);
        }
        if self.tick.is_multiple_of(VEHICLE_AI_EVERY) {
            self.world.vehicle_ai(&self.vehicles, &mut events);
        }
        mobility::step(&mut self.world, &self.vehicles, &self.map, &self.data.terrain, &mut events);
        self.tick += 1;
        self.update_perceived();
        events
    }

    fn apply(&mut self, c: &TimedCommand, events: &mut Vec<Event>) {
        match &c.cmd {
            Command::Move { unit, destination, speed_limit_mps } => {
                let Some(k) = self.world.index_of(*unit) else { return };
                if self.world.side[k] != c.side {
                    return; // a side can only order its own units
                }
                let Some(vi) = self.world.vehicle[k] else { return };
                let layer = self.map.mobility(&self.vehicles[vi].id).expect("checked at spawn");
                let pos = self.world.transform[k].pos;
                let cell = self.map.manifest.cell_size_m;
                match self.pathfinder.find(&layer.speed, cell, pos, *destination) {
                    Some(route) => {
                        let length = route.windows(2).map(|w| dist(w[0], w[1])).sum::<f64>();
                        self.world.set_route(k, route, *speed_limit_mps);
                        events.push(Event::RoutePlanned { unit: *unit, length_m: length });
                    }
                    None => events.push(Event::NoRoute { unit: *unit }),
                }
            }
            Command::Halt { unit } => {
                if let Some(k) = self.world.index_of(*unit)
                    && self.world.side[k] == c.side
                {
                    self.world.halt(k);
                    events.push(Event::Halted { unit: *unit });
                }
            }
        }
    }

    fn update_perceived(&mut self) {
        let world = &self.world;
        let vehicles = &self.vehicles;
        let tick = self.tick;
        for p in self.perceived.iter_mut() {
            p.rebuild(world, vehicles, tick);
        }
    }

    /// Hash of the full simulation state (golden replay tests, desync detection).
    pub fn state_hash(&self) -> u64 {
        let mut h = StableHasher::new();
        h.u64(self.tick);
        self.world.hash_into(&mut h);
        // RNG streams: hash their word positions so consumption shows up.
        for r in [&self.rngs.mobility, &self.rngs.sensors, &self.rngs.gunnery, &self.rngs.ai] {
            h.u64(r.get_word_pos() as u64);
        }
        h.finish()
    }
}

pub(crate) fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    libm::sqrt((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2))
}
