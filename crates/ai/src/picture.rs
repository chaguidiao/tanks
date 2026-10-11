//! What a side knows (DESIGN.md §6.2). The player UI and every AI read only these types;
//! they carry no ground-truth identities for enemy vehicles (CLAUDE.md rule 3).

pub type Vec2 = [f64; 2];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SideId(pub u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlatoonId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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
    pub condition: Condition,
    pub weapons: WeaponsStatus,
    pub task: Task,
    /// Remaining route, starting at the unit's position.
    pub route: Vec<Vec2>,
    /// Hull-down state reported by the crew for its current position, if set in a BP.
    pub position_note: Option<String>,
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

    /// Live (not reported destroyed) contacts.
    pub fn live_contacts(&self) -> impl Iterator<Item = &Contact> {
        self.contacts.iter().filter(|c| !c.destroyed)
    }
}
