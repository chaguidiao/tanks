use ai::picture::{ContactId, ContactLevel, Graphic, PlatoonId, TriggerCondition, WeaponsStatus};
use serde::{Deserialize, Serialize};
use sim_data::{Formation, Technique};

use crate::{EntityId, SideId, Vec2};

/// Who an order is addressed to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Addressee {
    Unit(EntityId),
    Platoon(PlatoonId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PositionKind {
    HullDown,
    TurretDown,
}

/// Orders into the simulation. Intent-level: platoon and vehicle AI decide how to execute them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Command {
    /// Move a single vehicle to a point by the fastest route, through checkpoints `via` in order
    /// (route with checkpoints, FM 1-02.2 / ADP 3-90 control measures).
    Move {
        unit: EntityId,
        destination: Vec2,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        via: Vec<Vec2>,
        speed_limit_mps: Option<f64>,
    },
    /// Halt in place, keeping the interrupted order (route, battle position) for `Resume`.
    Halt {
        to: Addressee,
    },
    /// Continue the order interrupted by the last `Halt`.
    Resume {
        to: Addressee,
    },
    /// Platoon movement in a formation using a movement technique (ATP 3-20.15 ch. 3), along a
    /// route through checkpoints `via` in order.
    PlatoonMove {
        platoon: PlatoonId,
        destination: Vec2,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        via: Vec<Vec2>,
        formation: Option<Formation>,
        technique: Technique,
    },
    SetFormation {
        platoon: PlatoonId,
        formation: Formation,
    },
    /// Occupy a battle position (ATP 3-20.15 ch. 5): circle area, threat direction (compass rad).
    OccupyPosition {
        platoon: PlatoonId,
        centre: Vec2,
        radius_m: f64,
        facing: f64,
        kind: PositionKind,
    },
    WeaponsControl {
        to: Addressee,
        status: WeaponsStatus,
    },
    /// Micro-order: engage a known contact.
    Engage {
        unit: EntityId,
        contact: ContactId,
    },
    /// Draw a control measure (not a radio order: no delay).
    AddGraphic {
        graphic: Graphic,
    },
    RemoveGraphic {
        id: u32,
    },
    /// Contingency order: `then` is executed without radio delay when the condition is met.
    AddTrigger {
        condition: TriggerCondition,
        then: Box<Command>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimedCommand {
    /// Tick the order was given.
    pub tick: u64,
    pub side: SideId,
    pub cmd: Command,
    /// Pre-planned (code-word / trigger / planning-phase) orders take effect without radio delay.
    pub immediate: bool,
    /// Issued by the simulation itself (commander AI, triggers): not re-injected on replay.
    #[serde(default)]
    pub generated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KillKind {
    Mobility,
    Firepower,
    Catastrophic,
    Crew,
}

/// Structured events out of the simulation. The client turns them into radio text (brevity,
/// grids, mils) in the display layer. Each event is addressed to one side.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    OrderReceived {
        side: SideId,
        to: Addressee,
    },
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
    Stuck {
        unit: EntityId,
    },
    OutOfFuel {
        unit: EntityId,
    },
    /// A spot report reached the side's picture.
    SpotReport {
        side: SideId,
        reporter: EntityId,
        contact: ContactId,
        level: ContactLevel,
        new: bool,
        range_m: f64,
    },
    /// Own vehicle fired.
    Fired {
        unit: EntityId,
        range_m: f64,
        from: Vec2,
        to: Vec2,
        hit: bool,
    },
    TriggerFired {
        side: SideId,
        id: u32,
    },
    /// Own vehicle observed its round strike.
    ShotResult {
        unit: EntityId,
        hit: bool,
        kill: Option<KillKind>,
    },
    /// Own vehicle was hit.
    Hit {
        unit: EntityId,
        penetrated: bool,
        kill: Option<KillKind>,
    },
    PlatoonReport {
        side: SideId,
        platoon: PlatoonId,
        report: PlatoonReport,
    },
    ScenarioEnd {
        winner: Option<SideId>,
        reason: EndReason,
    },
}

/// Structured platoon reports; the client renders them as radio text.
#[derive(Debug, Clone, PartialEq)]
pub enum PlatoonReport {
    SetAt {
        pos: Vec2,
    },
    NoRoute,
    SetInPosition {
        hull_down: u8,
        turret_down: u8,
        exposed: u8,
        facing: f64,
    },
    /// Battle drill "react to contact": halted and engaging towards `toward`.
    Contact {
        at: Vec2,
        toward: Vec2,
    },
    ContinuingMission,
    Bounding {
        section: u8,
    },
    /// Losses: members remaining.
    Strength {
        remaining: u8,
        of: u8,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EndReason {
    /// The side lost its victory-condition fraction of tanks.
    ForceDestroyed { side: SideId },
    /// The attacker seized and held the objective.
    ObjectiveSeized { side: SideId },
    /// Time ran out; `holder` holds the objective (if any).
    TimeLimit { holder: Option<SideId> },
}
