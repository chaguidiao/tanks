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
    /// Formation move along a planned route through checkpoints `via`. `checkpoints` = arc
    /// length of each checkpoint on `route`; `progress` = arc length reached by each section's
    /// virtual leader; `active` = bounding section.
    Move {
        destination: Vec2,
        via: Vec<Vec2>,
        checkpoints: Vec<f64>,
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
    /// Move to resume after a contact drill: (destination, checkpoints not yet passed, technique).
    pub resume: Option<(Vec2, Vec<Vec2>, Technique)>,
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
        let (route, waypoints) = match &self.order {
            PlatoonOrder::Move { destination, via, checkpoints, route, progress, .. } => {
                let s = progress[0].min(progress[1]);
                let mut wp: Vec<Vec2> = via.iter().zip(checkpoints).filter(|(_, c)| **c > s).map(|(v, _)| *v).collect();
                if checkpoints.len() != via.len() {
                    wp = via.clone(); // not planned yet
                }
                wp.push(*destination);
                let r = if route.is_empty() { Vec::new() } else { ai::formation::remaining(route, s) };
                (r, wp)
            }
            PlatoonOrder::Contact => match &self.resume {
                Some((d, via, _)) => (Vec::new(), via.iter().copied().chain(std::iter::once(*d)).collect()),
                None => (Vec::new(), Vec::new()),
            },
            _ => (Vec::new(), Vec::new()),
        };
        PlatoonView {
            route,
            waypoints,
            id: self.id,
            callsign: self.callsign.clone(),
            members: self.members.iter().map(|&k| w.id[k]).collect(),
            formation: self.formation,
            technique: self.technique,
            task,
        }
    }
}
