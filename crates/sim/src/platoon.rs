//! Platoons: membership, order state and the platoon view. Behaviour is in `platoon_ai.rs`.

use ai::picture::{ContactReaction, PlatoonId, PlatoonView, SideId};
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
    /// March speed limit (m/s) set by the last move or speed order; None = formation speed.
    pub speed_limit: Option<f64>,
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
    /// Order interrupted by a HALT, continued by RESUME. Any new order discards it.
    pub held: Option<PlatoonOrder>,
    pub quiet_since: u64,
    pub last_strength: u8,
    /// Tick of the last "react to indirect fire" report.
    pub last_indirect: Option<u64>,
    /// Breaking contact: the react-to-contact halt is suppressed until the move ends.
    pub withdraw: bool,
    /// Checkpoint plan being executed (DESIGN.md §8.3).
    pub plan: Option<crate::plan::PlanState>,
    pub contact_reaction: ContactReaction,
}

impl PlatoonOrder {
    /// A fresh formation move (planned on the next platoon tick).
    pub fn new_move(destination: Vec2, via: Vec<Vec2>) -> Self {
        PlatoonOrder::Move {
            destination,
            via,
            checkpoints: Vec::new(),
            route: Vec::new(),
            progress: [0.0, 0.0],
            active: 0,
        }
    }

    /// Destination and the checkpoints not yet passed by the lead section.
    pub fn move_ahead(&self) -> Option<(Vec2, Vec<Vec2>)> {
        let PlatoonOrder::Move { destination, via, checkpoints, progress, .. } = self else { return None };
        let ahead = if checkpoints.len() == via.len() {
            via.iter().zip(checkpoints).filter(|(_, s)| **s > progress[0]).map(|(v, _)| *v).collect()
        } else {
            via.clone() // not planned yet
        };
        Some((*destination, ahead))
    }

    /// Remaining route of the virtual leader and the waypoints still ahead (checkpoints, then
    /// the destination) for display.
    fn route_view(&self) -> (Vec<Vec2>, Vec<Vec2>) {
        let PlatoonOrder::Move { destination, via, checkpoints, route, progress, .. } = self else {
            return (Vec::new(), Vec::new());
        };
        let s = progress[0].min(progress[1]);
        let mut wp: Vec<Vec2> = via.iter().zip(checkpoints).filter(|(_, c)| **c > s).map(|(v, _)| *v).collect();
        if checkpoints.len() != via.len() {
            wp = via.clone(); // not planned yet
        }
        wp.push(*destination);
        let r = if route.is_empty() { Vec::new() } else { ai::formation::remaining(route, s) };
        (r, wp)
    }
}

impl Platoon {
    /// HALT (ATP 3-20.15 ch. 3, a halt during movement): stop in place but keep the interrupted
    /// order, so RESUME continues it from where the platoon stopped. Halting again keeps the
    /// order already held.
    pub fn halt(&mut self) {
        let held = match &self.order {
            // Kept as is: the route and the sections' progress along it are still valid.
            PlatoonOrder::Move { .. } => Some(self.order.clone()),
            // Not yet set: positions are chosen again on resume.
            PlatoonOrder::Occupy { centre, radius, facing, kind, reported: false, .. } => Some(PlatoonOrder::Occupy {
                centre: *centre,
                radius: *radius,
                facing: *facing,
                kind: *kind,
                assigned: false,
                reported: false,
            }),
            PlatoonOrder::Contact => self.resume.take().map(|(d, via, t)| {
                self.technique = t;
                PlatoonOrder::new_move(d, via)
            }),
            _ => None,
        };
        if held.is_some() {
            self.held = held;
        }
        self.order = PlatoonOrder::None;
        self.withdraw = false;
    }

    pub fn alive_members(&self, w: &World) -> Vec<usize> {
        self.members.iter().copied().filter(|&k| w.alive(k)).collect()
    }

    pub fn view(&self, w: &World) -> PlatoonView {
        let task = match &self.order {
            PlatoonOrder::None if matches!(&self.plan, Some(st) if matches!(st.phase, crate::plan::Phase::Waiting { .. })) =>
            {
                format!("WAITING AT CP {}", self.plan.as_ref().map_or(0, |st| st.cursor + 1))
            }
            PlatoonOrder::None => match &self.held {
                Some(PlatoonOrder::Move { .. }) => "HALTED, ROUTE HELD".to_owned(),
                Some(_) => "HALTED, BP ORDER HELD".to_owned(),
                None => "HALTED".to_owned(),
            },
            PlatoonOrder::Move { .. } => match &self.plan {
                Some(st) => format!("PLAN CP {}/{}", st.cursor + 1, st.plan.steps.len()),
                None => "MOVING".to_owned(),
            },
            PlatoonOrder::Occupy { reported: true, .. } => "SET IN BATTLE POSITION".to_owned(),
            PlatoonOrder::Occupy { .. } => "OCCUPYING BATTLE POSITION".to_owned(),
            PlatoonOrder::Contact => "IN CONTACT".to_owned(),
        };
        let (route, waypoints) = match (&self.order, &self.held) {
            (PlatoonOrder::Move { .. }, _) => self.order.route_view(),
            (PlatoonOrder::Contact, _) => match &self.resume {
                Some((d, via, _)) => (Vec::new(), via.iter().copied().chain(std::iter::once(*d)).collect()),
                None => (Vec::new(), Vec::new()),
            },
            (PlatoonOrder::None, Some(h)) => h.route_view(),
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
            held: self.held.is_some(),
            speed_limit: self.speed_limit,
            plan: self.plan.as_ref().map(|s| s.view()),
            contact_reaction: self.contact_reaction,
        }
    }
}
