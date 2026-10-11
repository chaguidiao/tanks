//! What a side knows. The player UI and every side's AI read only this (CLAUDE.md rule 3).
//!
//! M1: own units are reported exactly and instantly (digital position reporting). Contacts,
//! report delays and era-dependent own-position lag arrive with the sensor model in M2.

use sim_data::VehicleDef;

use crate::world::{Order, World};
use crate::{EntityId, SideId, Vec2, dist};

#[derive(Debug, Clone, PartialEq)]
pub enum Task {
    Idle,
    Moving { remaining_m: f64 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct OwnUnit {
    pub id: EntityId,
    pub callsign: String,
    pub vehicle_name: String,
    pub pos: Vec2,
    pub heading: f64,
    pub speed: f64,
    pub fuel_fraction: f64,
    pub task: Task,
    /// Remaining route, starting at the unit's position.
    pub route: Vec<Vec2>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PerceivedWorld {
    pub side: SideId,
    pub tick: u64,
    pub own: Vec<OwnUnit>,
}

impl PerceivedWorld {
    pub fn empty(side: SideId) -> Self {
        Self { side, tick: 0, own: Vec::new() }
    }

    pub fn unit(&self, id: EntityId) -> Option<&OwnUnit> {
        self.own.iter().find(|u| u.id == id)
    }

    pub(crate) fn rebuild(&mut self, w: &World, vehicles: &[VehicleDef], tick: u64) {
        self.tick = tick;
        self.own.clear();
        for k in 0..w.len() {
            if w.side[k] != self.side {
                continue;
            }
            let t = &w.transform[k];
            let (speed, fuel_fraction, name) = match (&w.mobility[k], w.vehicle[k]) {
                (Some(m), Some(vi)) => {
                    let d = &vehicles[vi];
                    (m.speed, m.fuel_l / d.mobility.fuel_capacity_l.get(), d.name.clone())
                }
                _ => (0.0, 0.0, String::new()),
            };
            let (task, route) = match w.ai[k].as_ref().map(|a| &a.order) {
                Some(Order::Move { route, segment, .. }) => {
                    let mut r = vec![t.pos];
                    r.extend_from_slice(&route[(*segment + 1).min(route.len())..]);
                    let remaining = r.windows(2).map(|p| dist(p[0], p[1])).sum();
                    (Task::Moving { remaining_m: remaining }, r)
                }
                _ => (Task::Idle, Vec::new()),
            };
            self.own.push(OwnUnit {
                id: w.id[k],
                callsign: w.callsign[k].clone(),
                vehicle_name: name,
                pos: t.pos,
                heading: t.heading,
                speed,
                fuel_fraction,
                task,
                route,
            });
        }
    }
}
