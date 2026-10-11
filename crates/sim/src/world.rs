//! Truth world: entities as struct-of-arrays with optional components (DESIGN.md §4.4), so
//! non-tank entity types can be added without touching vehicle code (CLAUDE.md rule 5).

use ai::drive::{self, DriveParams};
use sim_data::VehicleDef;
use terrain::BakedMap;
use terrain::hash::StableHasher;

use crate::{Event, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SideId(pub u8);

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
    /// Fuel remaining (L).
    pub fuel_l: f64,
    /// Seconds without progress while trying to move.
    pub stalled_s: f64,
    /// Commanded by the vehicle AI.
    pub cmd_speed: f64,
    pub cmd_heading: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Order {
    Idle,
    Move { route: Vec<Vec2>, segment: usize, speed_limit: Option<f64> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct VehicleAi {
    pub order: Order,
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

    #[allow(clippy::too_many_arguments)]
    pub fn spawn_vehicle(
        &mut self,
        side: SideId,
        callsign: &str,
        vehicle_index: usize,
        def: &VehicleDef,
        pos: Vec2,
        heading: f64,
        map: &BakedMap,
    ) -> EntityId {
        self.next_id += 1;
        let id = EntityId(self.next_id);
        self.id.push(id);
        self.side.push(side);
        self.callsign.push(callsign.to_owned());
        self.transform.push(Transform { pos, z: map.elevation.height(pos[0], pos[1]), heading, pitch: 0.0, roll: 0.0 });
        self.vehicle.push(Some(vehicle_index));
        self.mobility.push(Some(MobilityState {
            speed: 0.0,
            fuel_l: def.mobility.fuel_capacity_l.get(),
            stalled_s: 0.0,
            cmd_speed: 0.0,
            cmd_heading: heading,
        }));
        self.ai.push(Some(VehicleAi { order: Order::Idle }));
        id
    }

    pub fn set_route(&mut self, k: usize, route: Vec<Vec2>, speed_limit: Option<f64>) {
        if let Some(ai) = &mut self.ai[k] {
            ai.order = Order::Move { route, segment: 0, speed_limit };
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

    /// Vehicle AI tick (5 Hz): turn the current order into speed/heading commands.
    pub(crate) fn vehicle_ai(&mut self, vehicles: &[VehicleDef], events: &mut Vec<Event>) {
        for k in 0..self.len() {
            let (Some(ai), Some(m), Some(vi)) = (&mut self.ai[k], &mut self.mobility[k], self.vehicle[k]) else {
                continue;
            };
            let t = &self.transform[k];
            match &mut ai.order {
                Order::Idle => {
                    m.cmd_speed = 0.0;
                }
                Order::Move { route, segment, speed_limit } => {
                    let mv = &vehicles[vi].mobility;
                    let max = speed_limit.unwrap_or(f64::INFINITY).min(mv.max_speed_road_mps.get());
                    let p = DriveParams {
                        max_speed: max,
                        max_lateral_accel: mv.max_lateral_accel_mps2.get(),
                        braking_decel: mv.braking_decel_mps2.get(),
                        pivot_threshold: 1.2,
                    };
                    let c = drive::drive(t.pos, t.heading, m.speed, route, *segment, &p);
                    *segment = c.segment;
                    m.cmd_speed = c.speed;
                    m.cmd_heading = c.heading;
                    if c.arrived {
                        ai.order = Order::Idle;
                        m.cmd_speed = 0.0;
                        events.push(Event::Arrived { unit: self.id[k] });
                    }
                }
            }
        }
    }

    pub(crate) fn hash_into(&self, h: &mut StableHasher) {
        for k in 0..self.len() {
            h.u32(self.id[k].0);
            h.u8(self.side[k].0);
            let t = &self.transform[k];
            for v in [t.pos[0], t.pos[1], t.z, t.heading, t.pitch, t.roll] {
                h.f64(v);
            }
            if let Some(m) = &self.mobility[k] {
                for v in [m.speed, m.fuel_l, m.stalled_s, m.cmd_speed, m.cmd_heading] {
                    h.f64(v);
                }
            }
            if let Some(ai) = &self.ai[k] {
                match &ai.order {
                    Order::Idle => h.u8(0),
                    Order::Move { route, segment, .. } => {
                        h.u8(1);
                        h.u64(*segment as u64);
                        h.u64(route.len() as u64);
                    }
                }
            }
        }
    }
}
