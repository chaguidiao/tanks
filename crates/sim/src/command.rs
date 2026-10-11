use crate::{EntityId, SideId, Vec2};

/// Orders into the simulation. Intent-level: the vehicle AI decides how to execute them.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Move to a point by the fastest route on the planning grid.
    Move {
        unit: EntityId,
        destination: Vec2,
        speed_limit_mps: Option<f64>,
    },
    Halt {
        unit: EntityId,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimedCommand {
    pub tick: u64,
    pub side: SideId,
    pub cmd: Command,
}

/// Structured events out of the simulation. The client turns them into radio text (brevity,
/// grids, mils) in the display layer.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    RoutePlanned {
        unit: EntityId,
        length_m: f64,
    },
    NoRoute {
        unit: EntityId,
    },
    Arrived {
        unit: EntityId,
    },
    Halted {
        unit: EntityId,
    },
    /// Could not make progress (slope beyond traction, bogged) for several seconds.
    Stuck {
        unit: EntityId,
    },
    OutOfFuel {
        unit: EntityId,
    },
}

impl Event {
    pub fn unit(&self) -> EntityId {
        match *self {
            Event::RoutePlanned { unit, .. }
            | Event::NoRoute { unit }
            | Event::Arrived { unit }
            | Event::Halted { unit }
            | Event::Stuck { unit }
            | Event::OutOfFuel { unit } => unit,
        }
    }
}
