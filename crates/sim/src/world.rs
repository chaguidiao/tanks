//! Truth world: entities as struct-of-arrays with optional components (DESIGN.md §4.4), so
//! non-tank entity types can be added without touching vehicle code (CLAUDE.md rule 5).

use ai::picture::{Condition, PlatoonId, WeaponsStatus};
use sim_data::{SensorKind, VehicleDef};
use terrain::hash::StableHasher;

use crate::Vec2;

pub use ai::picture::{EntityId, SideId};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    /// Local map metres (x east, y north).
    pub pos: Vec2,
    /// Ground elevation at `pos` (m).
    pub z: f64,
    /// Compass heading of the hull (rad, clockwise from north).
    pub heading: f64,
    /// Hull pitch (rad, nose up positive) and roll (rad, right side down positive).
    pub pitch: f64,
    pub roll: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MobilityState {
    /// Signed speed along the hull axis (m/s).
    pub speed: f64,
    pub fuel_l: f64,
    pub stalled_s: f64,
    /// Commanded by the vehicle AI.
    pub cmd_speed: f64,
    pub cmd_heading: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Order {
    Idle,
    /// Drive a route; on arrival turn to `face` (if any) and report.
    Move {
        route: Vec<Vec2>,
        segment: usize,
        speed_limit: Option<f64>,
        face: Option<f64>,
        platoon_slot: bool,
    },
    /// Turn in place to a heading.
    Face {
        heading: f64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct VehicleAi {
    pub order: Order,
    /// What the crew reports its position as (e.g. "HULL-DOWN") once set.
    pub position_note: Option<String>,
    pub set: bool,
}

/// A vehicle's own picture of an enemy (crew level, before reporting).
#[derive(Debug, Clone, PartialEq)]
pub struct LocalTrack {
    /// World index of the observed entity (truth-side bookkeeping; never exported).
    pub target: usize,
    pub level: ai::picture::ContactLevel,
    pub last_seen_tick: u64,
    pub last_report_tick: u64,
    pub contact: Option<ai::picture::ContactId>,
    /// Last visible fraction and range (gunnery uses them).
    pub visible_fraction: f64,
    pub range_m: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SensorState {
    /// Sensors available at the scenario date: (kind, cycles per mrad, max range m).
    pub sensors: Vec<(SensorKind, f64, f64)>,
    pub tracks: Vec<LocalTrack>,
    /// Sights degraded by a hit (cycles halved).
    pub degraded: bool,
}

impl SensorState {
    pub fn track(&self, target: usize) -> Option<&LocalTrack> {
        self.tracks.iter().find(|t| t.target == target)
    }
}

#[derive(Debug, Default, Clone)]
pub struct World {
    next_id: u32,
    pub id: Vec<EntityId>,
    pub side: Vec<SideId>,
    pub callsign: Vec<String>,
    pub transform: Vec<Transform>,
    /// Index into the sim's vehicle definition table.
    pub vehicle: Vec<Option<usize>>,
    pub mobility: Vec<Option<MobilityState>>,
    pub ai: Vec<Option<VehicleAi>>,
    pub sensors: Vec<Option<SensorState>>,
    pub gun: Vec<Option<crate::gunnery::GunState>>,
    pub condition: Vec<Condition>,
    pub weapons: Vec<WeaponsStatus>,
    pub platoon: Vec<Option<PlatoonId>>,
    /// Tick of the last main-gun shot (detection cue).
    pub last_fired: Vec<Option<u64>>,
}

impl World {
    pub fn len(&self) -> usize {
        self.id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.id.is_empty()
    }

    pub fn index_of(&self, id: EntityId) -> Option<usize> {
        // Ids are allocated in increasing order and never removed, so the vector is sorted.
        self.id.binary_search(&id).ok()
    }

    pub fn alive(&self, k: usize) -> bool {
        self.condition[k] != Condition::Destroyed
    }

    pub fn can_move(&self, k: usize) -> bool {
        matches!(self.condition[k], Condition::Operational | Condition::FirepowerKill)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn spawn_vehicle(
        &mut self,
        side: SideId,
        callsign: &str,
        vehicle_index: usize,
        def: &VehicleDef,
        pos: Vec2,
        z: f64,
        heading: f64,
        year: i32,
        ammo: u32,
    ) -> EntityId {
        self.next_id += 1;
        let id = EntityId(self.next_id);
        self.id.push(id);
        self.side.push(side);
        self.callsign.push(callsign.to_owned());
        self.transform.push(Transform { pos, z, heading, pitch: 0.0, roll: 0.0 });
        self.vehicle.push(Some(vehicle_index));
        self.mobility.push(Some(MobilityState {
            speed: 0.0,
            fuel_l: def.mobility.fuel_capacity_l.get(),
            stalled_s: 0.0,
            cmd_speed: 0.0,
            cmd_heading: heading,
        }));
        self.ai.push(Some(VehicleAi { order: Order::Idle, position_note: None, set: false }));
        let sensors = def
            .sensors
            .iter()
            .filter(|s| s.in_service.contains(year))
            .map(|s| (s.kind, s.cycles_per_mrad.get(), s.max_range_m.as_ref().map_or(f64::INFINITY, |r| r.get())))
            .collect();
        self.sensors.push(Some(SensorState { sensors, tracks: Vec::new(), degraded: false }));
        self.gun.push(Some(crate::gunnery::GunState::new(heading, ammo)));
        self.condition.push(Condition::Operational);
        self.weapons.push(WeaponsStatus::Free);
        self.platoon.push(None);
        self.last_fired.push(None);
        id
    }

    pub fn set_route(&mut self, k: usize, route: Vec<Vec2>, speed_limit: Option<f64>, face: Option<f64>, slot: bool) {
        if let Some(ai) = &mut self.ai[k] {
            ai.order = Order::Move { route, segment: 0, speed_limit, face, platoon_slot: slot };
            ai.set = false;
            ai.position_note = None;
        }
        if let Some(m) = &mut self.mobility[k] {
            m.stalled_s = 0.0;
        }
    }

    pub fn halt(&mut self, k: usize) {
        if let Some(ai) = &mut self.ai[k] {
            ai.order = Order::Idle;
        }
        if let Some(m) = &mut self.mobility[k] {
            m.cmd_speed = 0.0;
        }
    }

    pub(crate) fn hash_into(&self, h: &mut StableHasher) {
        for k in 0..self.len() {
            h.u32(self.id[k].0);
            h.u8(self.side[k].0);
            h.u8(self.condition[k] as u8);
            let t = &self.transform[k];
            for v in [t.pos[0], t.pos[1], t.z, t.heading, t.pitch, t.roll] {
                h.f64(v);
            }
            if let Some(m) = &self.mobility[k] {
                for v in [m.speed, m.fuel_l, m.stalled_s, m.cmd_speed, m.cmd_heading] {
                    h.f64(v);
                }
            }
            if let Some(s) = &self.sensors[k] {
                h.u64(s.tracks.len() as u64);
                for t in &s.tracks {
                    h.u64(t.target as u64);
                    h.u8(t.level as u8);
                }
            }
            if let Some(g) = &self.gun[k] {
                g.hash_into(h);
            }
            if let Some(ai) = &self.ai[k] {
                match &ai.order {
                    Order::Idle => h.u8(0),
                    Order::Move { route, segment, .. } => {
                        h.u8(1);
                        h.u64(*segment as u64);
                        h.u64(route.len() as u64);
                    }
                    Order::Face { heading } => {
                        h.u8(2);
                        h.f64(*heading);
                    }
                }
            }
        }
    }
}
