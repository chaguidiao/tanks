//! Platoons: membership and the platoon view. Platoon AI (formations, techniques, battle
//! positions, fire distribution, drills) lives in `platoon_ai.rs`.

use ai::picture::{PlatoonId, PlatoonView, SideId};
use sim_data::{Formation, Technique};

use crate::Vec2;
use crate::command::PositionKind;
use crate::world::World;

#[derive(Debug, Clone, PartialEq)]
pub enum PlatoonOrder {
    None,
    Move {
        destination: Vec2,
        route: Vec<Vec2>,
        progress_m: f64,
        bound: u32,
    },
    Occupy {
        centre: Vec2,
        radius: f64,
        facing: f64,
        kind: PositionKind,
        assigned: bool,
    },
    /// React to contact: halted, fighting from current positions.
    Contact,
}

#[derive(Debug, Clone)]
pub struct Platoon {
    pub id: PlatoonId,
    pub side: SideId,
    pub callsign: String,
    /// World indices in platoon order (1 = leader).
    pub members: Vec<usize>,
    pub formation: Formation,
    pub technique: Technique,
    pub order: PlatoonOrder,
    /// Leader heading used to orient formation slots (compass rad).
    pub heading: f64,
    /// Last tick slot routes were re-planned.
    pub replanned: u64,
    pub contact_reported: bool,
}

impl Platoon {
    pub fn alive_members(&self, w: &World) -> Vec<usize> {
        self.members.iter().copied().filter(|&k| w.alive(k)).collect()
    }

    pub fn view(&self, w: &World) -> PlatoonView {
        let task = match &self.order {
            PlatoonOrder::None => "HALTED".to_owned(),
            PlatoonOrder::Move { .. } => format!("MOVING ({:?}, {:?})", self.formation, self.technique).to_uppercase(),
            PlatoonOrder::Occupy { kind, .. } => format!("OCCUPY BP ({kind:?})").to_uppercase(),
            PlatoonOrder::Contact => "IN CONTACT".to_owned(),
        };
        PlatoonView {
            id: self.id,
            callsign: self.callsign.clone(),
            members: self.members.iter().map(|&k| w.id[k]).collect(),
            formation: self.formation,
            technique: self.technique,
            task,
        }
    }
}
