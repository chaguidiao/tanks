//! What a side knows (DESIGN.md §6.2). The player UI and every AI read only these types;
//! they carry no ground-truth identities for enemy vehicles (CLAUDE.md rule 3).

use serde::{Deserialize, Serialize};

pub type Vec2 = [f64; 2];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EntityId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SideId(pub u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PlatoonId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ContactId(pub u32);

#[derive(Debug, Clone, PartialEq)]
pub enum Task {
    Idle,
    Moving { remaining_m: f64 },
    Overwatch,
    Occupying,
    Set,
    Engaging,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum WeaponsStatus {
    Hold,
    Tight,
    Free,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Condition {
    Operational,
    MobilityKill,
    FirepowerKill,
    Destroyed,
}

/// What the gun crew is doing (own units only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GunActivity {
    Ready,
    Laying,
    Lasing,
    Reloading,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OwnUnit {
    pub id: EntityId,
    pub callsign: String,
    pub vehicle_id: String,
    pub vehicle_name: String,
    pub platoon: Option<PlatoonId>,
    pub pos: Vec2,
    pub heading: f64,
    pub turret: f64,
    pub speed: f64,
    pub fuel_fraction: f64,
    pub ammo: u32,
    /// Rounds left per type: (type label, count), in loadout order.
    pub rounds: Vec<(String, u32)>,
    /// Round type the commander ordered; None = crew's choice.
    pub ammo_selected: Option<String>,
    /// Reactive armour tiles left.
    pub era_left: u32,
    pub condition: Condition,
    pub weapons: WeaponsStatus,
    pub task: Task,
    /// Remaining route, starting at the unit's position (also while halted with it held).
    pub route: Vec<Vec2>,
    /// Halted with its own (single-vehicle) route held for RESUME.
    pub held: bool,
    /// Speed limit (m/s) of its own (single-vehicle) move, current or held.
    pub speed_limit: Option<f64>,
    /// Hull-down state reported by the crew for its current position, if set in a BP.
    pub position_note: Option<String>,
    /// Crew fit for duty, of the vehicle's establishment.
    pub crew: (u8, u8),
    /// Sights damaged by a hit.
    pub sights_degraded: bool,
    pub gun: GunActivity,
    /// The contact the gun is engaging (the crew's own track, as reported).
    pub target: Option<ContactId>,
    /// Smoke grenade salvos left.
    pub smoke_salvos: u32,
    /// Engine smoke: None = the vehicle has none, Some(on).
    pub engine_smoke: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ContactLevel {
    /// Something is there.
    Detected,
    /// A tank (vs. other vehicle).
    Recognised,
    /// Which type of tank.
    Identified,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Contact {
    pub id: ContactId,
    /// Reported position (with reporting error).
    pub pos: Vec2,
    pub level: ContactLevel,
    /// Vehicle type name once identified.
    pub type_name: Option<String>,
    pub first_report_s: f64,
    pub last_report_s: f64,
    pub reported_by: EntityId,
    /// Reported destroyed by an own unit.
    pub destroyed: bool,
}

/// A shot this side knows about: its own, or one from a known contact.
#[derive(Debug, Clone, PartialEq)]
pub struct ShotView {
    pub from: Vec2,
    pub to: Vec2,
    pub time_s: f64,
    pub hit: bool,
    pub own: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlatoonView {
    pub id: PlatoonId,
    pub callsign: String,
    pub members: Vec<EntityId>,
    pub formation: sim_data::Formation,
    pub technique: sim_data::Technique,
    pub task: String,
    /// Remaining planned route of the platoon's virtual leader (empty when not moving).
    pub route: Vec<Vec2>,
    /// Checkpoints not yet passed, then the destination (also kept while in a contact drill
    /// or halted with the route held).
    pub waypoints: Vec<Vec2>,
    /// Halted with an interrupted order that RESUME continues.
    pub held: bool,
    /// March speed limit (m/s), None = formation speed.
    pub speed_limit: Option<f64>,
}

/// Control measures (DESIGN.md §8.4; MIL-STD-2525D tactical graphics).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GraphicKind {
    PhaseLine { points: Vec<Vec2> },
    Objective { centre: Vec2, radius: f64 },
    Trp { at: Vec2 },
    BattlePosition { centre: Vec2, radius: f64, facing: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Graphic {
    /// Assigned by the simulation (ignored when submitted).
    pub id: u32,
    pub label: String,
    pub kind: GraphicKind,
}

/// Condition of a contingency order (code word / trigger, DESIGN.md §8.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TriggerCondition {
    /// A reported enemy is across the phase line (on the friendly side of it).
    EnemyCrosses { line: u32 },
    /// The platoon's centroid has crossed the phase line.
    FriendlyCrosses { line: u32, platoon: PlatoonId },
    /// At a scenario time (s).
    AtTime { time_s: f64 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct TriggerView {
    pub id: u32,
    pub condition: TriggerCondition,
    /// Short description of the order it releases.
    pub order: String,
    pub fired: bool,
}

/// A smoke cloud in view (clouds carry no owner; anyone can see one).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmokeView {
    pub pos: Vec2,
    pub radius: f64,
    /// 0..1: opacity relative to a fully developed cloud.
    pub strength: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PerceivedWorld {
    pub side: SideId,
    pub tick: u64,
    pub time_s: f64,
    pub own: Vec<OwnUnit>,
    pub platoons: Vec<PlatoonView>,
    pub contacts: Vec<Contact>,
    pub shots: Vec<ShotView>,
    pub graphics: Vec<Graphic>,
    pub triggers: Vec<TriggerView>,
    pub smoke: Vec<SmokeView>,
}

impl PerceivedWorld {
    pub fn empty(side: SideId) -> Self {
        Self {
            side,
            tick: 0,
            time_s: 0.0,
            own: Vec::new(),
            platoons: Vec::new(),
            contacts: Vec::new(),
            shots: Vec::new(),
            graphics: Vec::new(),
            triggers: Vec::new(),
            smoke: Vec::new(),
        }
    }

    pub fn unit(&self, id: EntityId) -> Option<&OwnUnit> {
        self.own.iter().find(|u| u.id == id)
    }

    pub fn platoon(&self, id: PlatoonId) -> Option<&PlatoonView> {
        self.platoons.iter().find(|p| p.id == id)
    }

    pub fn contact(&self, id: ContactId) -> Option<&Contact> {
        self.contacts.iter().find(|c| c.id == id)
    }

    pub fn graphic(&self, id: u32) -> Option<&Graphic> {
        self.graphics.iter().find(|g| g.id == id)
    }

    /// Live (not reported destroyed) contacts.
    pub fn live_contacts(&self) -> impl Iterator<Item = &Contact> {
        self.contacts.iter().filter(|c| !c.destroyed)
    }
}
