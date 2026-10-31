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
    /// Rounds carried when full (all types): what resupply fills to.
    pub full_load: u32,
    /// Stage in a service-station resupply, when its platoon has that order (M8c).
    pub service: Option<ServiceStage>,
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
    /// Crew suppression 0..1 (rises under fire).
    pub suppression: f64,
    /// Set for a squad (plan M8B-INFANTRY §7); vehicle fields are then empty.
    pub squad: Option<SquadView>,
}

/// What a squad's posture looks like to its own side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SquadPosture {
    Mounted,
    Moving,
    Halted,
    DugIn,
    InBuilding,
}

impl SquadPosture {
    pub fn label(self) -> &'static str {
        match self {
            SquadPosture::Mounted => "MOUNTED",
            SquadPosture::Moving => "MOVING",
            SquadPosture::Halted => "HALTED",
            SquadPosture::DugIn => "DUG IN",
            SquadPosture::InBuilding => "IN BUILDING",
        }
    }
}

/// An own squad's state.
#[derive(Debug, Clone, PartialEq)]
pub struct SquadView {
    pub type_id: String,
    pub type_name: String,
    /// Men fit to fight, of the establishment.
    pub strength: (u8, u8),
    pub posture: SquadPosture,
    /// Carrier it rides in or belongs to.
    pub carrier: Option<EntityId>,
    pub building: Option<u32>,
    /// Below the combat-effective threshold.
    pub ineffective: bool,
    /// Bursts left, all weapons.
    pub bursts: u32,
    /// Rockets and missiles (plan M8B-AT-TEAMS §7).
    pub anti_armour: Vec<LauncherView>,
}

/// One anti-armour weapon type of a squad and its rounds left.
#[derive(Debug, Clone, PartialEq)]
pub struct LauncherView {
    pub name: String,
    pub rounds: u32,
    /// Guided missile (an anti-tank missile team) rather than a rocket.
    pub missile: bool,
}

/// Where a vehicle is in a service-station resupply (M8c; FM 17-15 (1996) service-station method).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceStage {
    /// Driving into the supply point.
    ToPoint,
    /// In the holding line until a station is free.
    Waiting,
    /// At the point, taking on fuel and ammunition.
    Served,
    /// Full, or the point has nothing more it needs.
    Done,
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

/// What a contact is, known once recognised (plan M8B-INFANTRY §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContactKind {
    Vehicle,
    Infantry,
    /// An unarmed supply vehicle (truck), recognised as such (M8c tailgate resupply).
    Supply,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Contact {
    pub id: ContactId,
    /// Reported position (with reporting error).
    pub pos: Vec2,
    pub level: ContactLevel,
    /// Vehicle or infantry, once recognised.
    pub kind: Option<ContactKind>,
    /// Vehicle type name once identified.
    pub type_name: Option<String>,
    pub first_report_s: f64,
    pub last_report_s: f64,
    pub reported_by: EntityId,
    /// Reported destroyed by an own unit.
    pub destroyed: bool,
    /// An anti-tank weapon: identified as a missile team, or seen launching (plan M8B-AT-TEAMS §4).
    pub anti_armour: bool,
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
    pub plan: Option<PlanView>,
    /// Step ids reached by the plan that last finished (for `PlatoonAt` synchronisation).
    pub plan_done: Vec<u16>,
    pub contact_reaction: ContactReaction,
    pub ambush_reaction: AmbushReaction,
    pub built_up: BuiltUp,
    /// Tick of the last close ambush the platoon reacted to.
    pub ambushed_tick: Option<u64>,
    /// Orders waiting in the queue.
    pub queued: u8,
    /// Supply point (id in `supply_points`) of a service-station resupply in progress.
    pub resupply_at: Option<u32>,
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
    /// Plan only: seconds since the step started waiting (a trigger never fires on it).
    Elapsed { s: f64 },
    /// The side has sent code word `word` (plans: since the wait began). FM 6-0 execution by code word.
    CodeWord { word: u8 },
    /// A recently reported enemy is within `range_m` of the platoon.
    ContactWithin { platoon: PlatoonId, range_m: f64 },
    /// A recently reported enemy is inside the area of an objective, battle position or TRP.
    EnemyInArea { graphic: u32 },
    /// The platoon has at most `remaining` vehicles left (not destroyed).
    StrengthAtMost { platoon: PlatoonId, remaining: u32 },
    /// A vehicle of the platoon is suppressed, i.e. taking fire.
    UnderFire { platoon: PlatoonId },
    /// Rockets or missiles were fired at the platoon from close range (plans: since the step
    /// began; elsewhere: in the last 30 s).
    Ambushed { platoon: PlatoonId },
    /// Plan only: platoon `platoon` has reached (is at or has passed) its plan step `step`.
    PlatoonAt { platoon: PlatoonId, step: u16 },
}

/// A code word the side has sent (latest transmission).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodeWordView {
    pub word: u8,
    pub tick: u64,
}

/// Battle-position posture (ATP 3-20.15 ch. 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PositionKind {
    HullDown,
    TurretDown,
}

/// Ammunition for a call for fire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FireKind {
    /// High explosive, fire for effect.
    He,
    Smoke,
}

/// A checkpoint plan for a platoon (DESIGN.md §8.3): a route whose legs and checkpoints carry
/// their own instructions (FM 1-02.2 checkpoints; FM 6-0 execution by code word).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub steps: Vec<PlanStep>,
}

impl Plan {
    pub fn index_of(&self, id: u16) -> Option<usize> {
        self.steps.iter().position(|s| s.id == id)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanStep {
    /// Stable id, unique within the plan (survives edits; used by `start` and branches).
    pub id: u16,
    pub at: Vec2,
    /// Settings for the leg that ends at this checkpoint.
    #[serde(default)]
    pub leg: LegSettings,
    /// Done on arrival, in order.
    #[serde(default)]
    pub actions: Vec<StepAction>,
    /// Hold here until met (None = continue at once).
    #[serde(default)]
    pub wait: Option<ConditionSet>,
    /// Done when the wait is met, before departing.
    #[serde(default)]
    pub on_release: Vec<StepAction>,
    /// On the leg to this checkpoint or waiting at it: if met, jump to step `goto`.
    #[serde(default)]
    pub branch: Option<Branch>,
}

impl PlanStep {
    pub fn new(id: u16, at: Vec2) -> Self {
        Self {
            id,
            at,
            leg: LegSettings::default(),
            actions: Vec::new(),
            wait: None,
            on_release: Vec::new(),
            branch: None,
        }
    }

    /// The platoon halts here (the formation does not simply flow through).
    pub fn is_stop(&self) -> bool {
        !self.actions.is_empty() || self.wait.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SpeedSetting {
    /// As fast as the formation allows.
    Max,
    Limit(f64),
}

/// None = keep what the platoon has.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LegSettings {
    #[serde(default)]
    pub speed: Option<SpeedSetting>,
    #[serde(default)]
    pub formation: Option<sim_data::Formation>,
    #[serde(default)]
    pub technique: Option<sim_data::Technique>,
    #[serde(default)]
    pub weapons: Option<WeaponsStatus>,
    #[serde(default)]
    pub on_contact: Option<ContactReaction>,
    #[serde(default)]
    pub on_ambush: Option<AmbushReaction>,
    #[serde(default)]
    pub built_up: Option<BuiltUp>,
}

/// What a platoon does when rockets or missiles are fired at it from close range (plan
/// M8F1-INFANTRY-AI §2; ATP 3-21.8 (2016) battle drill react to ambush (near)).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AmbushReaction {
    /// Smoke, reverse out of the kill zone with the front to the shooter, then fight.
    #[default]
    ReverseOut,
    /// Smoke and drive on out of the kill zone at full speed, returning fire.
    DriveThrough,
    /// The contact drill: halt, face the threat, engage.
    Halt,
}

/// Whether tank routes go through built-up ground (plan M8F1-INFANTRY-AI §3; FM 3-06.11: tanks
/// do not enter built-up areas without dismounted infantry).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuiltUp {
    /// Routes go around built-up ground where they can.
    #[default]
    Avoid,
    Enter,
}

/// What a platoon does when it meets the enemy (battle drills, ATP 3-20.15 App. F).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContactReaction {
    /// Doctrine-pack drill: halt, orient, engage, continue when quiet.
    #[default]
    Drill,
    /// Keep moving, return fire on the move (bypass).
    Continue,
    /// Find hasty hull-down positions near the current location facing the threat.
    HastyPosition,
    /// Battle drill break contact.
    BreakContact,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StepAction {
    Face {
        bearing: f64,
    },
    OccupyBp {
        radius_m: f64,
        facing: f64,
        kind: PositionKind,
    },
    SmokeScreen,
    EngineSmoke(bool),
    FireSmoke {
        at: Vec2,
        rounds: u32,
    },
    CallForFire {
        battery: u32,
        at: Vec2,
        kind: FireKind,
        volleys: u32,
    },
    Report,
    /// Carrier platoon: its squads leave by the ramp; the plan holds until the drill is over.
    Dismount,
    /// Carrier platoon: its squads walk back and board; the plan holds until they are aboard.
    Mount,
    /// Dismount platoon: occupy the building at (or nearest to) the checkpoint; the plan holds
    /// until the squads are inside or stopped.
    OccupyBuilding,
    /// Dismount platoon: occupy buildings within `radius_m` of the checkpoint that cover the
    /// direction `facing`, one squad to a building where there are enough (plan
    /// M8F1-INFANTRY-AI §5).
    OccupyBuildings {
        radius_m: f64,
        facing: f64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConditionSet {
    /// All conditions must hold (else any one).
    pub all: bool,
    pub conditions: Vec<TriggerCondition>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Branch {
    pub when: ConditionSet,
    /// Step id to continue with.
    pub goto: u16,
}

/// A platoon's plan as the side knows it.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanView {
    pub plan: Plan,
    /// Id of the step being executed (heading to, or acting/waiting at).
    pub cursor_id: u16,
    /// Waiting at the cursor step for this.
    pub waiting: Option<ConditionSet>,
    /// Ids of the steps already passed.
    pub passed: Vec<u16>,
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

/// What a battery is doing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BatteryStatus {
    Ready,
    /// A mission is under way: aiming at `at`, with `volleys_left` still to fire (0 = last rounds in flight).
    Mission {
        at: Vec2,
        smoke: bool,
        volleys_left: u32,
        impact_in_s: Option<f64>,
    },
    /// Moving to a new position after firing; ready again in `ready_in_s`.
    Displacing {
        ready_in_s: f64,
    },
}

/// A battery available to the side (called fire support).
#[derive(Debug, Clone, PartialEq)]
pub struct BatteryView {
    pub id: u32,
    pub callsign: String,
    pub name: String,
    pub pos: Vec2,
    pub tubes: u32,
    pub he_rounds: u32,
    pub smoke_rounds: u32,
    pub min_range_m: f64,
    pub max_range_m: f64,
    pub status: BatteryStatus,
    /// Seconds until the ammunition section is back from the trains, if it is away (C3).
    pub restock_in_s: Option<f64>,
    /// Basic loads still held in the trains for this battery.
    pub resupplies_left: u32,
}

/// A supply point of the side (M8c): where platoons are sent to resupply, with what is left.
#[derive(Debug, Clone, PartialEq)]
pub struct SupplyPointView {
    pub id: u32,
    pub callsign: String,
    pub pos: Vec2,
    pub unlimited: bool,
    pub fuel_l: f64,
    /// (round label, rounds left) for every round type stocked.
    pub rounds: Vec<(String, u32)>,
    pub smoke_salvos: u32,
}

/// A shell burst seen or heard by the side.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImpactView {
    pub pos: Vec2,
    pub time_s: f64,
    pub smoke: bool,
}

/// Sky, visibility and wind as the side's crews would judge them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WeatherView {
    pub label: String,
    pub sun_elevation_deg: f64,
    /// Optical effectiveness 0..1.
    pub light: f64,
    pub visibility_m: f64,
    pub precipitation_mm_h: f64,
    /// Soil traction relative to dry going (1 = dry).
    pub traction_factor: f64,
    pub wind_from_deg: f64,
    pub wind_speed_mps: f64,
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
    pub weather: WeatherView,
    pub fire_support: Vec<BatteryView>,
    pub supply_points: Vec<SupplyPointView>,
    pub impacts: Vec<ImpactView>,
    pub code_words: Vec<CodeWordView>,
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
            weather: WeatherView::default(),
            fire_support: Vec::new(),
            supply_points: Vec::new(),
            impacts: Vec::new(),
            code_words: Vec::new(),
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
