//! Platoons: membership, order state and the platoon view. Behaviour is in `platoon_ai.rs`.

use ai::picture::{PlatoonId, PlatoonView, SideId};
use ai::position::Candidate;
use sim_data::{Formation, Technique};

use crate::Vec2;
use crate::command::PositionKind;
use crate::world::World;

#[derive(Debug, Clone, PartialEq)]
pub enum PlatoonOrder {
    None,
    /// Formation move along a planned route. `progress` = arc length reached by each
    /// section's virtual leader; `active` = bounding section.
    Move {
        destination: Vec2,
        route: Vec<Vec2>,
        progress: [f64; 2],
        active: usize,
    },
    Occupy {
        centre: Vec2,
        radius: f64,
        facing: f64,
        kind: PositionKind,
        assigned: bool,
        reported: bool,
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
    /// Formation heading (compass rad).
    pub heading: f64,
    /// Tick slot routes were last re-planned.
    pub replanned: u64,
    pub contact_reported: bool,
    /// Last slot point each member was routed to.
    pub slot_targets: Vec<Vec2>,
    /// Battle-position assignment: (member world index, chosen position).
    pub bp: Vec<(usize, Candidate)>,
    /// Move to resume after a contact drill: (destination, technique).
    pub resume: Option<(Vec2, Technique)>,
    pub quiet_since: u64,
    pub last_strength: u8,
}

impl Platoon {
    pub fn alive_members(&self, w: &World) -> Vec<usize> {
        self.members.iter().copied().filter(|&k| w.alive(k)).collect()
    }

    pub fn view(&self, w: &World) -> PlatoonView {
        let task = match &self.order {
            PlatoonOrder::None => "HALTED".to_owned(),
            PlatoonOrder::Move { .. } => "MOVING".to_owned(),
            PlatoonOrder::Occupy { reported: true, .. } => "SET IN BATTLE POSITION".to_owned(),
            PlatoonOrder::Occupy { .. } => "OCCUPYING BATTLE POSITION".to_owned(),
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
