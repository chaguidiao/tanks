//! Vehicle AI (5 Hz): turns a vehicle's current order into speed and heading commands.
//!
//! Doctrine: ATP 3-20.15 (2025) ch. 3 "Tactical movement" for route following; on arrival in
//! a position the crew orients on the assigned direction (ch. 5, occupy a battle position).

use ai::drive::{self, DriveParams};
use sim_data::VehicleDef;

use crate::Event;
use crate::world::{Order, World};

/// Heading error below which a vehicle counts as oriented (rad).
const FACE_TOLERANCE: f64 = 0.04;

pub fn step(world: &mut World, vehicles: &[VehicleDef], events: &mut Vec<Event>) {
    for k in 0..world.len() {
        let movable = world.can_move(k);
        let (Some(ai), Some(m), Some(vi)) = (&mut world.ai[k], &mut world.mobility[k], world.vehicle[k]) else {
            continue;
        };
        if !movable {
            ai.order = Order::Idle;
            m.cmd_speed = 0.0;
            continue;
        }
        let t = &world.transform[k];
        match &mut ai.order {
            Order::Idle => m.cmd_speed = 0.0,
            Order::Face { heading } => {
                m.cmd_speed = 0.0;
                m.cmd_heading = *heading;
                if drive::wrap(*heading - t.heading).abs() < FACE_TOLERANCE {
                    ai.order = Order::Idle;
                    ai.set = true;
                }
            }
            Order::Move { route, segment, speed_limit, face, platoon_slot } => {
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
                    m.cmd_speed = 0.0;
                    let slot = *platoon_slot;
                    match *face {
                        Some(h) => ai.order = Order::Face { heading: h },
                        None => {
                            ai.order = Order::Idle;
                            ai.set = true;
                        }
                    }
                    if !slot {
                        events.push(Event::Arrived { unit: world.id[k] });
                    }
                }
            }
        }
    }
}
