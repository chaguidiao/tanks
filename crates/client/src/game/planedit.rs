//! Plan editing (docs/plans/archive/PLANS-AND-CONTEXT-MENU.md §4.3): a client draft of a platoon's
//! checkpoint plan, edited on the map (click to add, drag to move), with the context menu and
//! the step list, and sent as one `SetPlan` order (DESIGN.md §14 #22).
//!
//! Everything here reads the side's picture only (CLAUDE.md rule 3); ETAs use the map's
//! planning speed raster like the sim's route planner.

use std::collections::BTreeMap;

use sim::view::{
    AmbushReaction, Branch, BuiltUp, ConditionSet, ContactReaction, LegSettings, Plan, PlanStep, SpeedSetting,
    StepAction,
};
use terrain::path::{Pathfinder, segment_time};

use super::ctx::{MenuAct, reaction_name};
use super::*;
use crate::hud::MenuItem;

/// Pick radius of a checkpoint square and of a leg (px).
const CP_PICK_PX: f32 = 11.0;
const LEG_PICK_PX: f32 = 5.0;
/// Mid-leg labels are drawn when zoomed in closer than this (m/px).
const LABEL_MPP: f64 = 6.0;

/// A plan being edited.
#[derive(Debug, Clone)]
pub struct PlanDraft {
    pub platoon: PlatoonId,
    pub plan: Plan,
    /// Selected checkpoint (inspector, keyboard edits).
    pub sel: Option<u16>,
    /// Step the running plan was executing when the draft was made: SEND continues there.
    pub running: Option<u16>,
    /// Steps the running plan has passed (drawn dimmed).
    pub passed: Vec<u16>,
    /// START HERE: the step the sent plan begins with.
    pub start: Option<u16>,
    /// Undo and redo stacks of (plan, start), and the last state they were committed at.
    undo: Vec<Snap>,
    redo: Vec<Snap>,
    base: Snap,
}

/// A draft's undoable state.
type Snap = (Plan, Option<u16>);

impl PlanDraft {
    pub fn new(platoon: PlatoonId, plan: Plan, sel: Option<u16>, running: Option<u16>, passed: Vec<u16>) -> Self {
        let base = (plan.clone(), None);
        Self { platoon, plan, sel, running, passed, start: None, undo: Vec::new(), redo: Vec::new(), base }
    }

    /// Only positions: nothing a plain move order cannot say.
    pub fn is_plain(&self) -> bool {
        self.plan.steps.iter().all(|s| {
            s.leg == LegSettings::default()
                && s.actions.is_empty()
                && s.on_release.is_empty()
                && s.wait.is_none()
                && s.branch.is_none()
        })
    }

    /// Number of edits made since the draft was started (undoable ones).
    pub fn changes(&self) -> usize {
        self.undo.len()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Record the edits since the last call as one undo step.
    pub fn commit(&mut self) {
        if self.plan == self.base.0 && self.start == self.base.1 {
            return;
        }
        let prev = std::mem::replace(&mut self.base, (self.plan.clone(), self.start));
        self.undo.push(prev);
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn restore(&mut self, s: Snap) {
        self.plan = s.0.clone();
        self.start = s.1;
        self.base = s;
        if self.sel.is_none_or(|id| self.plan.index_of(id).is_none()) {
            self.sel = self.plan.steps.last().map(|st| st.id);
        }
    }

    /// Step back one edit. False = nothing to undo.
    pub fn undo(&mut self) -> bool {
        self.commit();
        let Some(prev) = self.undo.pop() else { return false };
        self.redo.push(self.base.clone());
        self.restore(prev);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else { return false };
        self.undo.push(self.base.clone());
        self.restore(next);
        true
    }

    fn next_id(&self) -> u16 {
        self.plan.steps.iter().map(|s| s.id).max().map_or(1, |m| m + 1)
    }

    fn step_mut(&mut self, id: u16) -> Option<&mut PlanStep> {
        self.plan.steps.iter_mut().find(|s| s.id == id)
    }

    /// Step the plan starts (or continues) with when sent.
    pub fn start_id(&self) -> Option<u16> {
        let has = |id: &u16| self.plan.index_of(*id).is_some();
        self.start.filter(has).or(self.running.filter(has)).or_else(|| self.plan.steps.first().map(|s| s.id))
    }
}

/// What the next map click gives a checkpoint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pick {
    Face,
    Bp(PositionKind),
    FireSmoke,
    CallFire(FireKind),
    Move,
}

impl Pick {
    pub fn prompt(self) -> &'static str {
        match self {
            Pick::Face => "CLICK THE DIRECTION TO FACE",
            Pick::Bp(_) => "CLICK THE THREAT DIRECTION OF THE BATTLE POSITION",
            Pick::FireSmoke => "CLICK THE SMOKE AIM POINT",
            Pick::CallFire(_) => "CLICK THE FIRE MISSION TARGET",
            Pick::Move => "CLICK THE NEW CP POSITION",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ActKind {
    Pick(Pick),
    SmokeScreen,
    EngineSmoke(bool),
    Report,
    Dismount,
    Mount,
    OccupyBuilding,
}

/// An edit from the context menu (applied to the draft, made from the running plan if needed).
#[derive(Debug, Clone, PartialEq)]
pub enum PlanEdit {
    /// Append a checkpoint at the menu point.
    /// Insert a checkpoint half way along the leg to `id`.
    InsertBefore(u16),
    /// Insert a checkpoint at the menu point, before `id` (leg menu).
    InsertHere(u16),
    Delete(u16),
    StartHere(u16),
    Move(u16),
    /// Cancel the platoon's plan.
    Clear,
    /// Start editing the platoon's plan.
    Edit(PlatoonId),
    Speed(u16, Option<SpeedSetting>),
    Formation(u16, Option<Formation>),
    Technique(u16, Option<Technique>),
    Weapons(u16, Option<WeaponsStatus>),
    OnContact(u16, Option<ContactReaction>),
    OnAmbush(u16, Option<AmbushReaction>),
    BuiltUp(u16, Option<BuiltUp>),
    /// Add an action at the checkpoint (`true` = on release).
    Act(u16, bool, ActKind),
    ClearActs(u16, bool),
    Wait(u16, Option<ConditionSet>),
    Branch(u16, Option<Branch>),
}

/// Plan step list rows → submenu path of the checkpoint menu.
pub fn row_path(k: u8) -> Vec<usize> {
    match k {
        0..=4 => vec![0, k as usize],
        k => vec![k as usize - 4],
    }
}

const ROW_NAMES: [&str; 9] =
    ["SPEED", "FORMATION", "TECHNIQUE", "WEAPONS", "ON CONTACT", "AT CP", "WAIT", "ON RELEASE", "IF ON LEG"];

/// How a plan is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Look {
    /// Being edited: dashed, draft colour.
    Draft,
    /// Sent, not yet acknowledged: dashed.
    Sent,
    /// Executing: solid.
    Running,
}

/// Leg times between fixed points (A* on the speed raster), keyed by vehicle type and the
/// points rounded to 10 m.
#[derive(Default)]
pub struct EtaCache {
    pf: Pathfinder,
    /// (vehicle type, from and to in 10 m units) → (raster time s, length m); None = no route.
    legs: BTreeMap<LegKey, Option<(f64, f64)>>,
}

type LegKey = (String, [i64; 4]);

/// (platoon, plan, step being executed, passed steps, look).
type ShownPlan<'a> = (PlatoonId, &'a Plan, Option<u16>, &'a [u16], Look);

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt()
}

fn speed_text(s: SpeedSetting) -> String {
    match s {
        SpeedSetting::Max => "MAX".into(),
        SpeedSetting::Limit(v) => format!("{} KM/H", format::kmh(v)),
    }
}

/// ON AMBUSH values (plan M8F1-INFANTRY-AI §2).
pub(super) const AMBUSH_REACTIONS: [AmbushReaction; 3] =
    [AmbushReaction::ReverseOut, AmbushReaction::DriveThrough, AmbushReaction::Halt];

pub(super) fn ambush_name(r: AmbushReaction) -> &'static str {
    match r {
        AmbushReaction::ReverseOut => "REVERSE OUT",
        AmbushReaction::DriveThrough => "DRIVE THROUGH",
        AmbushReaction::Halt => "HALT AND FIGHT",
    }
}

pub(super) fn ambush_short(r: AmbushReaction) -> &'static str {
    match r {
        AmbushReaction::ReverseOut => "REVERSE",
        AmbushReaction::DriveThrough => "THROUGH",
        AmbushReaction::Halt => "HALT",
    }
}

pub(super) fn built_up_name(b: BuiltUp) -> &'static str {
    match b {
        BuiltUp::Avoid => "AVOID",
        BuiltUp::Enter => "ENTER",
    }
}

pub(super) fn reaction_short(r: ContactReaction) -> &'static str {
    match r {
        ContactReaction::Drill => "DRILL",
        ContactReaction::Continue => "CONTINUE",
        ContactReaction::HastyPosition => "HASTY POS",
        ContactReaction::BreakContact => "BREAK",
    }
}

fn one(c: TriggerCondition) -> ConditionSet {
    ConditionSet { all: false, conditions: vec![c] }
}

impl Game {
    // ------------------------------------------------------------ draft state

    /// The plan shown for the selected platoon: (platoon, plan, step being executed, passed
    /// steps, look). The draft wins over a plan sent and not yet received, which wins over the
    /// executing one.
    pub(super) fn shown_plan(&self) -> Option<ShownPlan<'_>> {
        if let Some(d) = &self.plan {
            return Some((d.platoon, &d.plan, d.running, &d.passed, Look::Draft));
        }
        let p = self.sel_platoon?;
        if let Some(plan) = self.pending_plan(p) {
            return Some((p, plan, None, &[], Look::Sent));
        }
        let v = self.picture().platoon(p)?.plan.as_ref()?;
        Some((p, &v.plan, Some(v.cursor_id), &v.passed, Look::Running))
    }

    /// A plan sent to platoon `p` and not yet received.
    fn pending_plan(&self, p: PlatoonId) -> Option<&Plan> {
        self.graphics.iter().find_map(|g| match &g.kind {
            GraphicKind::Plan { plan } if g.platoon == Some(p) => Some(plan),
            _ => None,
        })
    }

    /// Start a draft for platoon `p` from the plan last sent to it, else the plan it executes,
    /// else its route, else empty.
    pub(super) fn plan_start(&mut self, p: PlatoonId) {
        if let Some(plan) = self.pending_plan(p).cloned() {
            let sel = plan.steps.first().map(|s| s.id);
            self.plan = Some(PlanDraft::new(p, plan, sel, None, Vec::new()));
            return;
        }
        self.plan = self.draft_from_picture(p);
    }

    /// A draft of the plan platoon `p` executes, else of its route.
    fn draft_from_picture(&self, p: PlatoonId) -> Option<PlanDraft> {
        let v = self.picture().platoon(p)?;
        Some(match &v.plan {
            Some(pv) => PlanDraft::new(p, pv.plan.clone(), Some(pv.cursor_id), Some(pv.cursor_id), pv.passed.clone()),
            None => {
                let steps: Vec<PlanStep> =
                    v.waypoints.iter().enumerate().map(|(i, &at)| PlanStep::new(i as u16 + 1, at)).collect();
                let sel = steps.last().map(|s| s.id);
                PlanDraft::new(p, Plan { steps }, sel, None, Vec::new())
            }
        })
    }

    /// Before a replace order for platoon `p`: keep what it replaces (its draft, else its sent
    /// or running plan, else its route) so Ctrl+Z can bring it back as a draft.
    pub(super) fn remember_replaced(&mut self, p: PlatoonId) {
        // A contingency does not replace anything yet.
        if self.order_mode != OrderMode::Now {
            return;
        }
        let old = if self.plan.as_ref().is_some_and(|d| d.platoon == p) {
            self.plan.take()
        } else if let Some(d) = self.parked.remove(&p) {
            Some(d)
        } else if let Some(plan) = self.pending_plan(p).cloned() {
            let sel = plan.steps.first().map(|s| s.id);
            Some(PlanDraft::new(p, plan, sel, None, Vec::new()))
        } else {
            self.draft_from_picture(p)
        };
        if let Some(d) = old.filter(|d| !d.plan.steps.is_empty()) {
            self.replaced.insert(p, d);
        }
        self.plan_drag = None;
        if matches!(self.tool, Tool::PlanCp | Tool::PlanPick { .. }) {
            self.tool = Tool::None;
        }
    }

    /// Keep each platoon's draft across selection changes: the selected platoon's draft is
    /// `self.plan`, the others wait in `self.parked`.
    pub(super) fn sync_drafts(&mut self) {
        if self.plan.as_ref().is_some_and(|d| Some(d.platoon) != self.sel_platoon)
            && let Some(d) = self.plan.take()
        {
            self.parked.insert(d.platoon, d);
            self.plan_drag = None;
            self.revert_armed = false;
            if matches!(self.tool, Tool::PlanCp | Tool::PlanPick { .. }) {
                self.tool = Tool::None;
            }
        }
        if self.plan.is_none()
            && let Some(p) = self.sel_platoon
        {
            self.plan = self.parked.remove(&p);
        }
        if !self.plan_drag.is_some_and(|d| d.2)
            && let Some(d) = &mut self.plan
        {
            d.commit();
        }
    }

    /// Does platoon `p` have a draft (shown or parked)?
    pub fn has_draft(&self, p: PlatoonId) -> bool {
        self.parked.contains_key(&p) || self.plan.as_ref().is_some_and(|d| d.platoon == p)
    }

    /// Ctrl+Z: undo a draft edit, else bring back the plan a replace order overwrote.
    pub(super) fn plan_undo(&mut self) {
        if self.plan.as_mut().is_some_and(|d| d.undo()) {
            self.plan_drag = None;
            return;
        }
        let Some(p) = self.sel_platoon else { return };
        if self.plan.is_none()
            && let Some(d) = self.replaced.remove(&p)
        {
            self.plan = Some(d);
            let cs = self.platoon_cs(p);
            self.radio(format!("{cs}: PREVIOUS PLAN RESTORED AS DRAFT. SEND TO ISSUE."));
        }
    }

    pub(super) fn plan_redo(&mut self) {
        if self.plan.as_mut().is_some_and(|d| d.redo()) {
            self.plan_drag = None;
        }
    }

    /// Make sure a draft exists for the selected platoon.
    pub(super) fn ensure_draft(&mut self) -> bool {
        if self.plan.is_none()
            && let Some(p) = self.sel_platoon
        {
            self.plan_start(p);
            if let Some(id) = self.selected_cp()
                && let Some(d) = &mut self.plan
                && d.plan.index_of(id).is_some()
            {
                d.sel = Some(id);
            }
        }
        self.plan.is_some()
    }

    /// The selected checkpoint: the draft's, else one selected on the running plan.
    pub(super) fn selected_cp(&self) -> Option<u16> {
        if let Some(d) = &self.plan {
            return d.sel;
        }
        self.sel_cp.filter(|(p, _)| Some(*p) == self.sel_platoon).map(|(_, id)| id)
    }

    /// Select a checkpoint. This never starts a draft.
    pub(super) fn select_cp(&mut self, id: u16) {
        match (&mut self.plan, self.sel_platoon) {
            (Some(d), _) => d.sel = Some(id),
            (None, Some(p)) => self.sel_cp = Some((p, id)),
            _ => {}
        }
    }

    /// REVERT: drop the selected platoon's draft. A draft with more than one edit asks first
    /// (a second REVERT confirms); Ctrl+Z / Esc never get here.
    pub(super) fn plan_revert_asked(&mut self) {
        let edits = self.plan.as_ref().map_or(0, |d| d.changes());
        if edits > 1 && !self.revert_armed {
            self.revert_armed = true;
            self.radio(format!("REVERT DROPS {edits} EDITS. PRESS REVERT AGAIN TO CONFIRM."));
            return;
        }
        self.plan_revert();
    }

    pub(super) fn plan_revert(&mut self) {
        self.revert_armed = false;
        self.plan = None;
        self.plan_drag = None;
        if matches!(self.tool, Tool::PlanCp | Tool::PlanPick { .. }) {
            self.tool = Tool::None;
        }
    }

    pub(super) fn plan_add_cp(&mut self, at: [f64; 2]) {
        if !self.ensure_draft() {
            return;
        }
        let d = self.plan.as_mut().expect("draft");
        let id = d.next_id();
        d.plan.steps.push(PlanStep::new(id, at));
        d.sel = Some(id);
    }

    pub(super) fn plan_delete(&mut self, id: u16) {
        let Some(d) = &mut self.plan else { return };
        let Some(i) = d.plan.index_of(id) else { return };
        d.plan.steps.remove(i);
        for s in &mut d.plan.steps {
            if s.branch.as_ref().is_some_and(|b| b.goto == id) {
                s.branch = None;
            }
        }
        let next = d.plan.steps.get(i).or(d.plan.steps.last()).map(|s| s.id);
        if d.running == Some(id) {
            d.running = d.plan.steps.get(i).map(|s| s.id);
        }
        if d.start == Some(id) {
            d.start = None;
        }
        d.sel = next;
    }

    pub(super) fn plan_step_sel(&mut self, next: bool) {
        if !self.ensure_draft() {
            return;
        }
        let d = self.plan.as_mut().expect("draft");
        let n = d.plan.steps.len();
        if n == 0 {
            return;
        }
        let i = d.sel.and_then(|id| d.plan.index_of(id));
        let j = match (i, next) {
            (None, _) => 0,
            (Some(i), true) => (i + 1) % n,
            (Some(i), false) => (i + n - 1) % n,
        };
        d.sel = Some(d.plan.steps[j].id);
    }

    /// A map point picked for checkpoint `id`.
    pub(super) fn plan_pick(&mut self, id: u16, release: bool, pick: Pick, world: [f64; 2]) {
        let battery = match pick {
            Pick::CallFire(k) => {
                self.fire_kind = k;
                self.fire_battery().map(|b| b.id)
            }
            _ => None,
        };
        let radius = self.bp_radius;
        let Some(s) = self.plan.as_mut().and_then(|d| d.step_mut(id)) else { return };
        let bearing = ai::drive::bearing(s.at, world);
        let act = match pick {
            Pick::Move => {
                s.at = world;
                return;
            }
            Pick::Face => StepAction::Face { bearing },
            Pick::Bp(kind) => StepAction::OccupyBp { radius_m: radius, facing: bearing, kind },
            Pick::FireSmoke => StepAction::FireSmoke { at: world, rounds: 1 },
            Pick::CallFire(kind) => {
                let Some(battery) = battery else { return };
                let volleys = if kind == FireKind::He { 3 } else { 1 };
                StepAction::CallForFire { battery, at: world, kind, volleys }
            }
        };
        let list = if release { &mut s.on_release } else { &mut s.actions };
        // One battle position / facing per checkpoint: a new one replaces the old.
        list.retain(|a| {
            !matches!(
                (a, &act),
                (StepAction::OccupyBp { .. }, StepAction::OccupyBp { .. })
                    | (StepAction::Face { .. }, StepAction::Face { .. })
            )
        });
        list.push(act);
    }

    /// SEND: the draft goes out as one order.
    pub(super) fn send_plan(&mut self) {
        let Some(d) = self.plan.take() else { return };
        self.plan_drag = None;
        if matches!(self.tool, Tool::PlanCp | Tool::PlanPick { .. }) {
            self.tool = Tool::None;
        }
        let Some(start) = d.start_id() else {
            self.radio("PLAN HAS NO CHECKPOINTS. NOTHING SENT.".into());
            return;
        };
        let p = d.platoon;
        if d.is_plain() && d.running.is_none() && d.start.is_none() && self.sel_platoon == Some(p) {
            let mut via: Vec<[f64; 2]> = d.plan.steps.iter().map(|s| s.at).collect();
            let dest = via.pop().expect("a start id means steps");
            self.issue_move_at(via, dest, self.current_speed());
            return;
        }
        let cs = self.platoon_cs(p);
        let n = d.plan.steps.len();
        let k = d.plan.index_of(start).map_or(1, |i| i + 1);
        let last = format::grid(d.plan.steps[n - 1].at);
        let mut words: Vec<String> = Vec::new();
        for s in &d.plan.steps {
            for c in s.wait.iter().flat_map(|w| w.conditions.iter()) {
                if let TriggerCondition::CodeWord { word } = c {
                    let w = self.code_word_name(*word);
                    if !words.contains(&w) {
                        words.push(w);
                    }
                }
            }
        }
        let words = if words.is_empty() { String::new() } else { format!(", CODE WORD {}", words.join(", ")) };
        let from = if k > 1 { format!(", FROM CP {k}") } else { String::new() };
        self.radio(format!("TO {cs}: PLAN, {n} CP{from}, LAST CP GRID {last}{words}."));
        self.remember_replaced(p);
        self.graphics.retain(|g| g.platoon != Some(p));
        self.graphics.push(OrderGraphic {
            platoon: Some(p),
            unit: None,
            kind: GraphicKind::Plan { plan: d.plan.clone() },
            acknowledged: false,
        });
        self.order(Command::SetPlan { platoon: p, plan: d.plan, start });
    }

    /// Release a code word: every plan and contingency waiting on it goes (FM 6-0).
    pub(super) fn send_code_word(&mut self, word: u8) {
        if self.sim.doctrine(self.side).is_none_or(|d| (word as usize) >= d.code_words.len()) {
            return;
        }
        if self.planning {
            self.radio("PLANNING PHASE: CODE WORDS ARE SENT AFTER H-HOUR.".into());
            return;
        }
        // A control signal: no radio delay; the net confirms it on the next tick.
        self.sim.submit(self.side, Command::CodeWord { word });
    }

    // ------------------------------------------------------------ map input

    /// Checkpoint square under screen point `s`.
    pub(super) fn plan_cp_at(&self, cam: &Camera, s: P2) -> Option<u16> {
        let (_, plan, ..) = self.shown_plan()?;
        plan.steps
            .iter()
            .map(|st| {
                let q = cam.to_screen(st.at);
                (st.id, (q[0] - s[0]).abs().max((q[1] - s[1]).abs()))
            })
            .filter(|(_, d)| *d <= CP_PICK_PX)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _)| id)
    }

    /// Leg (identified by the checkpoint it ends at) under screen point `s`.
    pub(super) fn plan_leg_at(&self, cam: &Camera, s: P2) -> Option<u16> {
        let (p, plan, running, passed, _) = self.shown_plan()?;
        let from = self.platoon_centre(p);
        let legs = plan_legs(plan, running, passed, from);
        legs.iter()
            .filter(|l| !l.passed)
            .find(|l| super::ctx::seg_dist(s, cam.to_screen(l.a), cam.to_screen(l.b)) < LEG_PICK_PX)
            .map(|l| l.id)
    }

    pub(super) fn platoon_centre(&self, p: PlatoonId) -> Option<[f64; 2]> {
        let pic = self.picture();
        let ps: Vec<[f64; 2]> = pic
            .platoon(p)?
            .members
            .iter()
            .filter_map(|m| pic.unit(*m))
            .filter(|u| u.condition != Condition::Destroyed)
            .map(|u| u.pos)
            .collect();
        (!ps.is_empty()).then(|| {
            let n = ps.len() as f64;
            [ps.iter().map(|p| p[0]).sum::<f64>() / n, ps.iter().map(|p| p[1]).sum::<f64>() / n]
        })
    }

    /// Left button on checkpoints: select, drag to move. True = the frame's click is used.
    pub(super) fn plan_mouse(&mut self, inp: &Input, cam: &Camera, over_ui: bool) -> bool {
        if let Some((id, from, moving)) = self.plan_drag {
            if inp.left_down {
                let far = ((inp.mouse[0] - from[0]).powi(2) + (inp.mouse[1] - from[1]).powi(2)).sqrt() > 4.0;
                if (moving || far) && self.ensure_draft() {
                    self.plan_drag = Some((id, from, true));
                    let w = cam.to_world(inp.mouse);
                    if let Some(s) = self.plan.as_mut().and_then(|d| d.step_mut(id)) {
                        s.at = w;
                    }
                }
                return true;
            }
            self.plan_drag = None;
            return moving;
        }
        if over_ui || !inp.left_pressed || !matches!(self.tool, Tool::None | Tool::PlanCp) {
            return false;
        }
        let Some(id) = self.plan_cp_at(cam, inp.mouse) else { return false };
        self.select_cp(id);
        self.plan_drag = Some((id, inp.mouse, false));
        true
    }

    /// Open a menu asked for by a key or a step-list row, now that the camera is known.
    pub(super) fn open_requested_menu(&mut self, cam: &Camera, inp: &Input) {
        let Some((target, path, at)) = self.menu_request.take() else { return };
        let world = match target {
            MenuTarget::PlanCp(id) => self.shown_plan().and_then(|(_, pl, ..)| pl.index_of(id).map(|i| pl.steps[i].at)),
            _ => None,
        };
        let at = at.unwrap_or_else(|| match world {
            Some(w) if path.is_empty() => {
                let s = cam.to_screen(w);
                [s[0] + 14.0, s[1] - 10.0]
            }
            _ => inp.mouse,
        });
        let world = world.unwrap_or_else(|| cam.to_world(at));
        let (title, items) = self.build_menu(target, world);
        let mut m = hud::PopupMenu::new(at, title, items);
        m.open_path(&path);
        self.menu = Some(m);
        self.menu_world = world;
        self.menu_target_id = target;
    }

    // ------------------------------------------------------------ menus

    fn pa(e: PlanEdit) -> MenuAct {
        MenuAct::Plan(e)
    }

    fn leg_items(&self, id: u16, leg: &sim::view::LegSettings) -> Vec<MenuItem<MenuAct>> {
        let keep = |set: bool, e: PlanEdit| MenuItem::new("KEEP", Self::pa(e)).checked(!set);
        let mut speed = vec![
            keep(leg.speed.is_some(), PlanEdit::Speed(id, None)),
            MenuItem::new("MAX", Self::pa(PlanEdit::Speed(id, Some(SpeedSetting::Max))))
                .checked(leg.speed == Some(SpeedSetting::Max)),
        ];
        for v in self.speed_presets().into_iter().flatten() {
            let s = SpeedSetting::Limit(v);
            speed.push(
                MenuItem::new(speed_text(s), Self::pa(PlanEdit::Speed(id, Some(s)))).checked(leg.speed == Some(s)),
            );
        }
        let mut form = vec![keep(leg.formation.is_some(), PlanEdit::Formation(id, None))];
        for f in Formation::ALL {
            form.push(
                MenuItem::new(format_formation(f), Self::pa(PlanEdit::Formation(id, Some(f))))
                    .checked(leg.formation == Some(f)),
            );
        }
        let mut tech = vec![keep(leg.technique.is_some(), PlanEdit::Technique(id, None))];
        for t in [Technique::Traveling, Technique::TravelingOverwatch, Technique::BoundingOverwatch] {
            tech.push(
                MenuItem::new(format_technique(t), Self::pa(PlanEdit::Technique(id, Some(t))))
                    .checked(leg.technique == Some(t)),
            );
        }
        let mut weap = vec![keep(leg.weapons.is_some(), PlanEdit::Weapons(id, None))];
        for w in [WeaponsStatus::Free, WeaponsStatus::Tight, WeaponsStatus::Hold] {
            weap.push(
                MenuItem::new(weapons_str(w), Self::pa(PlanEdit::Weapons(id, Some(w)))).checked(leg.weapons == Some(w)),
            );
        }
        let mut react = vec![keep(leg.on_contact.is_some(), PlanEdit::OnContact(id, None))];
        for r in [
            ContactReaction::Drill,
            ContactReaction::Continue,
            ContactReaction::HastyPosition,
            ContactReaction::BreakContact,
        ] {
            react.push(
                MenuItem::new(reaction_name(r), Self::pa(PlanEdit::OnContact(id, Some(r))))
                    .checked(leg.on_contact == Some(r)),
            );
        }
        let mut ambush = vec![keep(leg.on_ambush.is_some(), PlanEdit::OnAmbush(id, None))];
        for r in AMBUSH_REACTIONS {
            ambush.push(
                MenuItem::new(ambush_name(r), Self::pa(PlanEdit::OnAmbush(id, Some(r))))
                    .checked(leg.on_ambush == Some(r)),
            );
        }
        let mut built = vec![keep(leg.built_up.is_some(), PlanEdit::BuiltUp(id, None))];
        for b in [BuiltUp::Avoid, BuiltUp::Enter] {
            built.push(
                MenuItem::new(built_up_name(b), Self::pa(PlanEdit::BuiltUp(id, Some(b))))
                    .checked(leg.built_up == Some(b)),
            );
        }
        let or_keep = |s: Option<String>| s.unwrap_or_else(|| "KEEP".into());
        vec![
            MenuItem::submenu("SPEED", speed).key("S").value(or_keep(leg.speed.map(speed_text))),
            MenuItem::submenu("FORMATION", form).key("F").value(or_keep(leg.formation.map(format_formation))),
            MenuItem::submenu("TECHNIQUE", tech)
                .key("T")
                .value(or_keep(leg.technique.map(|t| short_technique(t).into()))),
            MenuItem::submenu("WEAPONS", weap).key("W").value(or_keep(leg.weapons.map(|w| weapons_str(w).into()))),
            MenuItem::submenu("ON CONTACT", react)
                .key("C")
                .value(or_keep(leg.on_contact.map(|r| reaction_short(r).into()))),
            MenuItem::submenu("ON AMBUSH", ambush)
                .key("A")
                .value(or_keep(leg.on_ambush.map(|r| ambush_short(r).into())))
                .tip("ROCKETS OR MISSILES FROM CLOSE RANGE: REVERSE OUT (DEFAULT), DRIVE THROUGH, OR HALT AND FIGHT"),
            MenuItem::submenu("BUILT-UP", built)
                .key("U")
                .value(or_keep(leg.built_up.map(|b| built_up_name(b).into())))
                .tip("TANK ROUTES AVOID BUILT-UP GROUND WITHOUT DISMOUNTS (DEFAULT) OR ENTER IT"),
        ]
    }

    fn action_items(&self, id: u16, at: [f64; 2], release: bool, acts: &[StepAction]) -> Vec<MenuItem<MenuAct>> {
        let act = |k: ActKind| Self::pa(PlanEdit::Act(id, release, k));
        let (squads, carries) = self.plan_platoon().map_or((false, false), |p| self.troop_roles(p));
        let clear = MenuItem::new("CLEAR", Self::pa(PlanEdit::ClearActs(id, release))).value(format!("{}", acts.len()));
        let clear = if acts.is_empty() { clear.disabled("NO ACTIONS") } else { clear };
        let fire = !self.picture().fire_support.is_empty();
        let cff = |key: &'static str| {
            let m = MenuItem::submenu(
                "CALL FOR FIRE AT",
                vec![
                    MenuItem::new("HE...", act(ActKind::Pick(Pick::CallFire(FireKind::He)))),
                    MenuItem::new("SMOKE...", act(ActKind::Pick(Pick::CallFire(FireKind::Smoke)))),
                ],
            )
            .key(key);
            if fire { m } else { m.disabled("NO BATTERY") }
        };
        if squads {
            // A dismount platoon (TODO G8): no turret, smoke grenades or hull-down positions.
            let near = self.sim.map().enterable_building_near(at, sim::plan::OCCUPY_BUILDING_RADIUS_M).is_some();
            let occ = MenuItem::new("OCCUPY BUILDING", act(ActKind::OccupyBuilding)).key("O");
            return vec![
                if near { occ } else { occ.disabled("NO BUILDING AT THE CP") },
                cff("Y"),
                MenuItem::new("REPORT", act(ActKind::Report)).key("R"),
                clear,
            ];
        }
        let smoke = self.smoke_rounds(self.plan_platoon().map(Addressee::Platoon)) > 0;
        let fs = MenuItem::new("FIRE SMOKE AT...", act(ActKind::Pick(Pick::FireSmoke))).key("N");
        let mut v = vec![
            MenuItem::new("FACE...", act(ActKind::Pick(Pick::Face))).key("F"),
            MenuItem::submenu(
                "OCCUPY BP",
                vec![
                    MenuItem::new("HULL-DOWN...", act(ActKind::Pick(Pick::Bp(PositionKind::HullDown)))),
                    MenuItem::new("TURRET-DOWN...", act(ActKind::Pick(Pick::Bp(PositionKind::TurretDown)))),
                ],
            )
            .key("B")
            .value(format!("{:.0} M", self.bp_radius)),
            MenuItem::new("SMOKE SCREEN", act(ActKind::SmokeScreen)).key("Z"),
            MenuItem::new("ENGINE SMOKE ON", act(ActKind::EngineSmoke(true))).key("E"),
            MenuItem::new("ENGINE SMOKE OFF", act(ActKind::EngineSmoke(false))),
            if smoke { fs } else { fs.disabled("NO SMOKE ROUNDS") },
            cff("Y"),
            MenuItem::new("REPORT", act(ActKind::Report)).key("R"),
        ];
        if carries {
            // ATP 3-21.71 ch. 3: dismount and pick-up points; the plan holds for the drill.
            v.push(MenuItem::new("DISMOUNT", act(ActKind::Dismount)).key("U"));
            v.push(MenuItem::new("MOUNT", act(ActKind::Mount)).key("K"));
        }
        v.push(clear);
        v
    }

    fn wait_items(&self, id: u16, p: PlatoonId, cur: &Option<ConditionSet>) -> Vec<MenuItem<MenuAct>> {
        let w = |c: TriggerCondition| {
            let set = one(c);
            let checked = cur.as_ref() == Some(&set);
            (Self::pa(PlanEdit::Wait(id, Some(set))), checked)
        };
        let item = |label: String, c: TriggerCondition| {
            let (a, ch) = w(c);
            MenuItem::new(label, a).checked(ch)
        };
        let pic = self.picture();
        let words: Vec<MenuItem<MenuAct>> = self
            .sim
            .doctrine(self.side)
            .map(|d| d.code_words.clone())
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(k, name)| item(name, TriggerCondition::CodeWord { word: k as u8 }))
            .collect();
        let fors: Vec<MenuItem<MenuAct>> = [1.0, 2.0, 5.0, 10.0, 15.0]
            .into_iter()
            .map(|m| item(format!("{m:.0} MIN"), TriggerCondition::Elapsed { s: m * 60.0 }))
            .collect();
        let now = self.sim.time_s();
        let start = self.scenario.start_time_s as f64;
        let untils: Vec<MenuItem<MenuAct>> = [5.0, 10.0, 15.0, 30.0, 60.0]
            .into_iter()
            .map(|m| {
                let t = ((now + m * 60.0) / 60.0).round() * 60.0;
                item(format!("{} (+{m:.0} MIN)", format::clock(start + t)), TriggerCondition::AtTime { time_s: t })
            })
            .collect();
        let lines: Vec<_> = pic
            .graphics
            .iter()
            .filter(|g| matches!(g.kind, SimGraphic::PhaseLine { .. }))
            .map(|g| item(g.label.clone(), TriggerCondition::EnemyCrosses { line: g.id }))
            .collect();
        let areas: Vec<_> = pic
            .graphics
            .iter()
            .filter(|g| !matches!(g.kind, SimGraphic::PhaseLine { .. }))
            .map(|g| item(g.label.clone(), TriggerCondition::EnemyInArea { graphic: g.id }))
            .collect();
        let mut plts = Vec::new();
        for v in pic.platoons.iter().filter(|v| v.id != p) {
            let Some(pv) = &v.plan else { continue };
            let cps = pv
                .plan
                .steps
                .iter()
                .enumerate()
                .map(|(i, s)| item(format!("CP {}", i + 1), TriggerCondition::PlatoonAt { platoon: v.id, step: s.id }))
                .collect();
            plts.push(MenuItem::submenu(format!("PLT {}", v.callsign), cps));
        }
        let sub = |label: &str, key: &'static str, v: Vec<MenuItem<MenuAct>>, why: &str| {
            let it = MenuItem::submenu(label, v).key(key);
            if it.enabled { it } else { it.disabled(why) }
        };
        vec![
            sub("CODE WORD", "W", words, "NO CODE WORDS IN THE DOCTRINE PACK"),
            sub("FOR", "F", fors, ""),
            sub("UNTIL", "U", untils, ""),
            sub("ENEMY CROSSES", "X", lines, "NO PHASE LINE DRAWN"),
            sub("ENEMY IN", "I", areas, "NO OBJECTIVE, BP OR TRP"),
            sub("PLT REACHES", "P", plts, "NO OTHER PLATOON HAS A PLAN"),
            MenuItem::new("NONE", Self::pa(PlanEdit::Wait(id, None))).key("N").checked(cur.is_none()),
        ]
    }

    fn branch_items(&self, id: u16, p: PlatoonId, plan: &Plan, cur: &Option<Branch>) -> Vec<MenuItem<MenuAct>> {
        let conds = [
            ("CONTACT < 1 KM", TriggerCondition::ContactWithin { platoon: p, range_m: 1000.0 }),
            ("CONTACT < 2 KM", TriggerCondition::ContactWithin { platoon: p, range_m: 2000.0 }),
            ("CONTACT < 3 KM", TriggerCondition::ContactWithin { platoon: p, range_m: 3000.0 }),
            ("UNDER FIRE", TriggerCondition::UnderFire { platoon: p }),
            ("AMBUSHED", TriggerCondition::Ambushed { platoon: p }),
            ("STRENGTH <= 3", TriggerCondition::StrengthAtMost { platoon: p, remaining: 3 }),
            ("STRENGTH <= 2", TriggerCondition::StrengthAtMost { platoon: p, remaining: 2 }),
        ];
        let mut v: Vec<MenuItem<MenuAct>> = conds
            .into_iter()
            .map(|(label, c)| {
                let when = one(c);
                let targets = plan
                    .steps
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.id != id)
                    .map(|(i, s)| {
                        let b = Branch { when: when.clone(), goto: s.id };
                        let ch = cur.as_ref() == Some(&b);
                        MenuItem::new(format!("GO TO CP {}", i + 1), Self::pa(PlanEdit::Branch(id, Some(b))))
                            .checked(ch)
                    })
                    .collect();
                let it = MenuItem::submenu(format!("IF {label}"), targets);
                if it.enabled { it } else { it.disabled("NO OTHER CP") }
            })
            .collect();
        v.push(MenuItem::new("NONE", Self::pa(PlanEdit::Branch(id, None))).key("N").checked(cur.is_none()));
        v
    }

    fn plan_platoon(&self) -> Option<PlatoonId> {
        self.shown_plan().map(|s| s.0)
    }

    /// Checkpoint menu (DESIGN.md §10.3; first-level rows match the step list).
    pub(super) fn plan_cp_menu(&self, id: u16) -> (String, Vec<MenuItem<MenuAct>>) {
        let Some((p, plan, _, _, _)) = self.shown_plan() else { return (String::new(), Vec::new()) };
        let Some(i) = plan.index_of(id) else { return (String::new(), Vec::new()) };
        let st = &plan.steps[i];
        let start = self.plan.as_ref().and_then(|d| d.start_id());
        let release = MenuItem::submenu("ON RELEASE", self.action_items(id, st.at, true, &st.on_release))
            .key("O")
            .value(self.actions_text(&st.on_release));
        let v = vec![
            MenuItem::submenu("LEG TO CP", self.leg_items(id, &st.leg)).key("L").value(leg_text(&st.leg)),
            MenuItem::submenu("AT CP", self.action_items(id, st.at, false, &st.actions))
                .key("A")
                .value(self.actions_text(&st.actions)),
            MenuItem::submenu("WAIT", self.wait_items(id, p, &st.wait))
                .key("W")
                .value(st.wait.as_ref().map_or("NONE".into(), |w| self.condition_set_text(w))),
            if st.wait.is_some() { release } else { release.disabled("NO WAIT SET") },
            MenuItem::submenu("IF ON LEG", self.branch_items(id, p, plan, &st.branch))
                .key("B")
                .value(st.branch.as_ref().map_or("NONE".into(), |b| self.branch_text(plan, b))),
            MenuItem::new("MOVE CP...", Self::pa(PlanEdit::Move(id))).key("M"),
            MenuItem::new("INSERT CP BEFORE", Self::pa(PlanEdit::InsertBefore(id))).key("I"),
            MenuItem::new("DELETE CP", Self::pa(PlanEdit::Delete(id))).key("D"),
            MenuItem::new("START HERE", Self::pa(PlanEdit::StartHere(id))).checked(start == Some(id)),
            MenuItem::new("CLEAR PLAN", Self::pa(PlanEdit::Clear)),
        ];
        (format!("CP {}  GRID {}", i + 1, format::grid(st.at)), v)
    }

    /// Leg menu: insert a checkpoint, or set the leg.
    pub(super) fn plan_leg_menu(&self, id: u16) -> (String, Vec<MenuItem<MenuAct>>) {
        let Some((_, plan, ..)) = self.shown_plan() else { return (String::new(), Vec::new()) };
        let Some(i) = plan.index_of(id) else { return (String::new(), Vec::new()) };
        let mut v = vec![MenuItem::new("INSERT CP HERE", Self::pa(PlanEdit::InsertHere(id))).key("I")];
        v.extend(self.leg_items(id, &plan.steps[i].leg));
        (format!("LEG TO CP {}", i + 1), v)
    }

    pub(super) fn run_plan_edit(&mut self, e: PlanEdit, at: [f64; 2]) {
        if let PlanEdit::Edit(p) = e {
            self.sel_platoon = Some(p);
            self.sel_unit = self.picture().platoon(p).and_then(|v| v.members.first().copied());
            self.draft.clear();
            self.sync_drafts();
            if self.plan.as_ref().is_none_or(|d| d.platoon != p) {
                self.plan_start(p);
            }
            self.tool = Tool::PlanCp;
            return;
        }
        if e == PlanEdit::Clear {
            let Some(p) = self.plan_platoon() else { return };
            self.remember_replaced(p);
            self.plan_revert();
            let cs = self.platoon_cs(p);
            self.radio(format!("TO {cs}: CANCEL PLAN."));
            self.order(Command::SetPlan { platoon: p, plan: Plan::default(), start: 0 });
            return;
        }
        if !self.ensure_draft() {
            return;
        }
        let centre = self.plan.as_ref().and_then(|d| self.platoon_centre(d.platoon));
        let d = self.plan.as_mut().expect("draft");
        match e {
            PlanEdit::InsertBefore(id) | PlanEdit::InsertHere(id) => {
                let Some(i) = d.plan.index_of(id) else { return };
                let pt = match e {
                    PlanEdit::InsertHere(_) => at,
                    _ => {
                        let prev = if i > 0 { Some(d.plan.steps[i - 1].at) } else { centre };
                        let b = d.plan.steps[i].at;
                        prev.map_or(b, |a| [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0])
                    }
                };
                let nid = d.next_id();
                d.plan.steps.insert(i, PlanStep::new(nid, pt));
                if d.running == Some(id) {
                    d.running = Some(nid);
                }
                d.sel = Some(nid);
            }
            PlanEdit::Delete(id) => self.plan_delete(id),
            PlanEdit::StartHere(id) => {
                d.start = Some(id);
                d.sel = Some(id);
            }
            PlanEdit::Move(id) => {
                d.sel = Some(id);
                self.tool = Tool::PlanPick { id, release: false, pick: Pick::Move };
            }
            PlanEdit::Act(id, release, ActKind::Pick(pick)) => {
                d.sel = Some(id);
                self.tool = Tool::PlanPick { id, release, pick };
            }
            e => {
                let Some(id) = (match &e {
                    PlanEdit::Speed(id, _)
                    | PlanEdit::Formation(id, _)
                    | PlanEdit::Technique(id, _)
                    | PlanEdit::Weapons(id, _)
                    | PlanEdit::OnContact(id, _)
                    | PlanEdit::OnAmbush(id, _)
                    | PlanEdit::BuiltUp(id, _)
                    | PlanEdit::Act(id, ..)
                    | PlanEdit::ClearActs(id, _)
                    | PlanEdit::Wait(id, _)
                    | PlanEdit::Branch(id, _) => Some(*id),
                    _ => None,
                }) else {
                    return;
                };
                d.sel = Some(id);
                let Some(s) = d.step_mut(id) else { return };
                match e {
                    PlanEdit::Speed(_, v) => s.leg.speed = v,
                    PlanEdit::Formation(_, v) => s.leg.formation = v,
                    PlanEdit::Technique(_, v) => s.leg.technique = v,
                    PlanEdit::Weapons(_, v) => s.leg.weapons = v,
                    PlanEdit::OnContact(_, v) => s.leg.on_contact = v,
                    PlanEdit::OnAmbush(_, v) => s.leg.on_ambush = v,
                    PlanEdit::BuiltUp(_, v) => s.leg.built_up = v,
                    PlanEdit::Act(_, release, k) => {
                        let a = match k {
                            ActKind::SmokeScreen => StepAction::SmokeScreen,
                            ActKind::EngineSmoke(on) => StepAction::EngineSmoke(on),
                            ActKind::Report => StepAction::Report,
                            ActKind::Dismount => StepAction::Dismount,
                            ActKind::Mount => StepAction::Mount,
                            ActKind::OccupyBuilding => StepAction::OccupyBuilding,
                            ActKind::Pick(_) => return,
                        };
                        if release { s.on_release.push(a) } else { s.actions.push(a) }
                    }
                    PlanEdit::ClearActs(_, release) => {
                        if release {
                            s.on_release.clear()
                        } else {
                            s.actions.clear()
                        }
                    }
                    PlanEdit::Wait(_, w) => {
                        if w.is_none() {
                            s.on_release.clear();
                        }
                        s.wait = w;
                    }
                    PlanEdit::Branch(_, b) => s.branch = b,
                    _ => {}
                }
            }
        }
    }

    // ------------------------------------------------------------ text

    pub(super) fn condition_text(&self, c: &TriggerCondition) -> String {
        let pic = self.picture();
        let label = |id: u32, or: &str| pic.graphic(id).map_or(or.to_owned(), |g| g.label.clone());
        match c {
            TriggerCondition::EnemyCrosses { line } => format!("ENEMY X {}", label(*line, "PL")),
            TriggerCondition::FriendlyCrosses { line, .. } => format!("OWN X {}", label(*line, "PL")),
            TriggerCondition::AtTime { time_s } => {
                format!("AT {}", format::clock(self.scenario.start_time_s as f64 + time_s))
            }
            TriggerCondition::Elapsed { s } => format!("{:.0} MIN", s / 60.0),
            TriggerCondition::CodeWord { word } => self.code_word_name(*word),
            TriggerCondition::ContactWithin { range_m, .. } => format!("CONTACT < {}", format::distance(*range_m)),
            TriggerCondition::EnemyInArea { graphic } => format!("ENEMY IN {}", label(*graphic, "AREA")),
            TriggerCondition::StrengthAtMost { remaining, .. } => format!("STRENGTH <= {remaining}"),
            TriggerCondition::UnderFire { .. } => "UNDER FIRE".into(),
            TriggerCondition::Ambushed { .. } => "AMBUSHED".into(),
            TriggerCondition::PlatoonAt { platoon, step } => {
                let v = pic.platoon(*platoon);
                let n = v
                    .and_then(|v| v.plan.as_ref())
                    .and_then(|pv| pv.plan.index_of(*step))
                    .map_or(format!("{step}"), |i| format!("{}", i + 1));
                format!("PLT {} AT CP {n}", v.map_or("?".into(), |v| v.callsign.clone()))
            }
        }
    }

    pub(super) fn condition_set_text(&self, s: &ConditionSet) -> String {
        let parts: Vec<String> = s.conditions.iter().map(|c| self.condition_text(c)).collect();
        parts.join(if s.all { " AND " } else { " OR " })
    }

    fn branch_text(&self, plan: &Plan, b: &Branch) -> String {
        let to = plan.index_of(b.goto).map_or("?".into(), |i| format!("{}", i + 1));
        format!("IF {} > CP {to}", self.condition_set_text(&b.when))
    }

    fn actions_text(&self, acts: &[StepAction]) -> String {
        if acts.is_empty() {
            return "NONE".into();
        }
        let v: Vec<String> = acts
            .iter()
            .map(|a| match a {
                StepAction::Face { bearing } => format!("FACE {}", format::mils(*bearing)),
                StepAction::OccupyBp { facing, kind, .. } => {
                    format!(
                        "BP {} {}",
                        if *kind == PositionKind::HullDown { "HD" } else { "TD" },
                        format::mils(*facing)
                    )
                }
                StepAction::SmokeScreen => "SMOKE".into(),
                StepAction::EngineSmoke(on) => format!("ENG SMK {}", if *on { "ON" } else { "OFF" }),
                StepAction::FireSmoke { .. } => "SMK RD".into(),
                StepAction::CallForFire { kind, .. } => {
                    format!("FIRE {}", if *kind == FireKind::He { "HE" } else { "SMK" })
                }
                StepAction::Report => "REPORT".into(),
                StepAction::Dismount => "DISMOUNT".into(),
                StepAction::Mount => "MOUNT".into(),
                StepAction::OccupyBuilding => "OCC BLDG".into(),
            })
            .collect();
        v.join(", ")
    }

    // ------------------------------------------------------------ ETA

    /// Estimated time of arrival at each step from now (TacOps): leg times on the planning speed
    /// raster, capped by the leg's march speed or the formation speed. After a wait with no time
    /// bound the times are relative to its release (`?+`). Empty strings for passed steps.
    pub(super) fn plan_etas(&self, p: PlatoonId, plan: &Plan, running: Option<u16>, passed: &[u16]) -> Vec<String> {
        let pic = self.picture();
        let mut out = vec![String::new(); plan.steps.len()];
        let Some(v) = pic.platoon(p) else { return out };
        let Some(from) = self.platoon_centre(p) else { return out };
        let alive: Vec<&OwnUnit> =
            v.members.iter().filter_map(|m| pic.unit(*m)).filter(|u| u.condition != Condition::Destroyed).collect();
        let Some(lead) = alive.first() else { return out };
        let ds = self.sim.data();
        let slowest = alive
            .iter()
            .filter_map(|u| ds.vehicle(&u.vehicle_id))
            .map(|d| d.mobility.max_speed_cross_country_mps.get())
            .fold(f64::INFINITY, f64::min);
        if !slowest.is_finite() {
            return out;
        }
        let map = self.sim.map();
        let Some(layer) = map.mobility(&lead.vehicle_id) else { return out };
        let cell = map.manifest.cell_size_m;
        let dragging = self.plan_drag.is_some_and(|d| d.2);
        let mut cache = self.eta.borrow_mut();
        if cache.legs.len() > 512 {
            cache.legs.clear();
        }
        let first = running.and_then(|id| plan.index_of(id)).unwrap_or(0);
        let mut cap = v.speed_limit;
        let mut t = 0.0;
        let mut known = true;
        let mut at = from;
        for (i, s) in plan.steps.iter().enumerate() {
            if passed.contains(&s.id) && i < first {
                continue;
            }
            match s.leg.speed {
                Some(SpeedSetting::Max) => cap = None,
                Some(SpeedSetting::Limit(l)) => cap = Some(l),
                None => {}
            }
            let vmax = (slowest * sim::platoon_ai::FORMATION_SPEED_FRACTION).min(cap.unwrap_or(f64::INFINITY));
            let leg = if i == first {
                // From where the platoon is now: a straight line (cheap; it moves every frame).
                let len = dist(at, s.at);
                Some((segment_time(&layer.speed, cell, at, s.at).unwrap_or(0.0), len))
            } else {
                let q = |x: f64| (x / 10.0).round() as i64;
                let key = (lead.vehicle_id.clone(), [q(at[0]), q(at[1]), q(s.at[0]), q(s.at[1])]);
                match cache.legs.get(&key) {
                    Some(r) => *r,
                    None if dragging => None,
                    None => {
                        let r = cache.pf.find(&layer.speed, cell, at, s.at).map(|path| {
                            let len: f64 = path.windows(2).map(|w| dist(w[0], w[1])).sum();
                            let rt: f64 = path
                                .windows(2)
                                .map(|w| segment_time(&layer.speed, cell, w[0], w[1]).unwrap_or(0.0))
                                .sum();
                            (rt, len)
                        });
                        cache.legs.insert(key, r);
                        r
                    }
                }
            };
            let Some((rt, len)) = leg else {
                out[i] = "ETA ?".into();
                known = false;
                at = s.at;
                t = 0.0;
                continue;
            };
            t += rt.max(len / vmax.max(0.5));
            let m = (t / 60.0).floor();
            out[i] = if known {
                format!("ETA {}", &format::clock(self.sim.clock_s() + t)[..5])
            } else {
                format!("?+{m:.0}:{:02.0}", t - m * 60.0)
            };
            // Waits: a timed one adds its time; any other makes later times relative.
            if let Some(w) = &s.wait {
                let bound = w.conditions.iter().filter_map(|c| match c {
                    TriggerCondition::Elapsed { s } => Some(*s),
                    TriggerCondition::AtTime { time_s } if known => Some((time_s - self.sim.time_s() - t).max(0.0)),
                    _ => None,
                });
                let all_timed = w
                    .conditions
                    .iter()
                    .all(|c| matches!(c, TriggerCondition::Elapsed { .. } | TriggerCondition::AtTime { .. }));
                // ALL: the last timed one, if nothing else is needed; ANY: the first (at the latest).
                let b = if w.all { bound.reduce(f64::max).filter(|_| all_timed) } else { bound.reduce(f64::min) };
                match b {
                    Some(x) => t += x,
                    None => {
                        known = false;
                        t = 0.0;
                    }
                }
            }
            at = s.at;
        }
        out
    }

    // ------------------------------------------------------------ drawing

    /// The selected platoon's plan: the draft, a sent one not yet received, or the executing one.
    pub(super) fn draw_selected_plan(&self, f: &mut Frame, cam: &Camera) {
        let Some((p, plan, running, passed, look)) = self.shown_plan() else { return };
        let sel = self.selected_cp();
        let eta = self.plan_etas(p, plan, running, passed);
        self.draw_plan_look(f, cam, p, plan, running, passed, look, sel, &eta);
    }

    /// A plan sent and not yet received (dashed).
    pub(super) fn draw_sent_plan(&self, f: &mut Frame, cam: &Camera, p: PlatoonId, plan: &Plan) {
        self.draw_plan_look(f, cam, p, plan, None, &[], Look::Sent, None, &[]);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_plan_look(
        &self,
        f: &mut Frame,
        cam: &Camera,
        p: PlatoonId,
        plan: &Plan,
        running: Option<u16>,
        passed: &[u16],
        look: Look,
        sel: Option<u16>,
        eta: &[String],
    ) {
        let c = if look == Look::Draft { style::ROUTE_DRAFT } else { style::FRIENDLY };
        let dim = Rgba(c.0, c.1, c.2, 80);
        let dashed = look != Look::Running;
        let legs = plan_legs(plan, running, passed, self.platoon_centre(p));
        for l in &legs {
            let col = if l.passed { dim } else { c };
            let (a, b) = (cam.to_screen(l.a), cam.to_screen(l.b));
            if dashed {
                route_line(f, cam, &[l.a, l.b], col, true);
            } else {
                f.line(a, b, 1.5, col);
            }
            let st = &plan.steps[l.idx];
            let len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
            // A march speed shows as ticks across the leg (not colour only).
            if matches!(st.leg.speed, Some(SpeedSetting::Limit(_))) && len > 30.0 && !l.passed {
                let (ux, uy) = ((b[0] - a[0]) / len, (b[1] - a[1]) / len);
                let mut d = 20.0;
                while d < len - 14.0 {
                    let q = [a[0] + ux * d, a[1] + uy * d];
                    f.line([q[0] - uy * 4.0, q[1] + ux * 4.0], [q[0] + uy * 4.0, q[1] - ux * 4.0], 1.0, col);
                    d += 28.0;
                }
            }
            let text = leg_text(&st.leg);
            if cam.mpp < LABEL_MPP && !l.passed && text != "KEEP" && len > 120.0 {
                let m = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
                let w = f.text_width(&text, style::FONT);
                f.rect(m[0] - w / 2.0 - 3.0, m[1] - 15.0, w + 6.0, 13.0, style::PANEL);
                f.text(&text, m[0] - w / 2.0, m[1] - 14.0, style::FONT, col);
            }
        }
        let cursor = running.and_then(|id| plan.index_of(id));
        for (i, st) in plan.steps.iter().enumerate() {
            let s = cam.to_screen(st.at);
            let gone = passed.contains(&st.id) && cursor.is_none_or(|k| i < k);
            let col = if gone { dim } else { c };
            // Battle position ordered at the checkpoint.
            for a in &st.actions {
                if let StepAction::OccupyBp { radius_m, facing, .. } = a {
                    symbols::battle_position(f, cam, st.at, *radius_m, *facing, "", look == Look::Running);
                }
                if let StepAction::CallForFire { at, .. } | StepAction::FireSmoke { at, .. } = a {
                    let q = cam.to_screen(*at);
                    f.circle_lines(q, 6.0, style::AMBER);
                    f.line([q[0] - 9.0, q[1]], [q[0] + 9.0, q[1]], 1.0, style::AMBER);
                    f.line([q[0], q[1] - 9.0], [q[0], q[1] + 9.0], 1.0, style::AMBER);
                    f.text(&format!("CP {}", i + 1), q[0] + 10.0, q[1] + 2.0, style::FONT, style::AMBER);
                }
            }
            // Numbered square; the selected one is ringed, the current one marked.
            f.rect(s[0] - 9.0, s[1] - 9.0, 18.0, 18.0, style::PANEL);
            f.rect_lines(s[0] - 9.0, s[1] - 9.0, 18.0, 18.0, col);
            if sel == Some(st.id) {
                f.rect_lines(s[0] - 12.0, s[1] - 12.0, 24.0, 24.0, style::SELECT);
            }
            if cursor == Some(i) && look == Look::Running {
                f.rect_lines(s[0] - 11.0, s[1] - 11.0, 22.0, 22.0, style::AMBER);
            }
            let t = format!("{}", i + 1);
            let tw = f.text_width(&t, style::FONT_S);
            f.text(&t, s[0] - tw / 2.0, s[1] - style::FONT_S as f32 / 2.0, style::FONT_S, col);
            if gone {
                continue;
            }
            // Badges: ETA, actions, wait, on release.
            let mut lines: Vec<(String, Rgba)> = Vec::new();
            if let Some(e) = eta.get(i).filter(|e| !e.is_empty()) {
                lines.push((e.clone(), style::TEXT_DIM));
            }
            if !st.actions.is_empty() {
                lines.push((self.actions_text(&st.actions), col));
            }
            if let Some(w) = &st.wait {
                lines.push((format!("WAIT {}", self.condition_set_text(w)), style::AMBER));
            }
            if !st.on_release.is_empty() {
                lines.push((format!("THEN {}", self.actions_text(&st.on_release)), col));
            }
            for (k, (l, lc)) in lines.iter().enumerate() {
                let y = s[1] - 8.0 + k as f32 * 12.0;
                let w = f.text_width(l, style::FONT);
                f.rect(s[0] + 12.0, y - 1.0, w + 4.0, 12.0, style::PANEL);
                f.text(l, s[0] + 14.0, y, style::FONT, *lc);
            }
            // Branch: a dotted arrow to the checkpoint it goes to.
            if let Some(b) = &st.branch
                && let Some(j) = plan.index_of(b.goto)
            {
                let (a, z) = (s, cam.to_screen(plan.steps[j].at));
                let len = ((z[0] - a[0]).powi(2) + (z[1] - a[1]).powi(2)).sqrt();
                if len > 24.0 {
                    let (ux, uy) = ((z[0] - a[0]) / len, (z[1] - a[1]) / len);
                    let mut d = 12.0;
                    while d < len - 12.0 {
                        f.circle([a[0] + ux * d, a[1] + uy * d], 1.2, style::AMBER);
                        d += 6.0;
                    }
                    let tip = [z[0] - ux * 12.0, z[1] - uy * 12.0];
                    for sgn in [-1.0f32, 1.0] {
                        let q = [tip[0] - ux * 7.0 + sgn * uy * 4.0, tip[1] - uy * 7.0 - sgn * ux * 4.0];
                        f.line(tip, q, 1.5, style::AMBER);
                    }
                    let m = [(a[0] + z[0]) / 2.0, (a[1] + z[1]) / 2.0];
                    let txt = format!("IF {}", self.condition_set_text(&b.when));
                    f.text(&txt, m[0] + 4.0, m[1] + 2.0, style::FONT, style::AMBER);
                }
            }
        }
    }

    // ------------------------------------------------------------ card and step list

    /// Plan rows of the selection panel: one per checkpoint, then the selected checkpoint's
    /// settings (each opens that part of its menu).
    pub(super) fn plan_rows(&self, x: f32, mut y: f32, w: f32) -> (Vec<Button<GameAction>>, f32) {
        let mut rows = Vec::new();
        let Some((p, plan, running, passed, look)) = self.shown_plan() else { return (rows, y) };
        if self.sel_platoon != Some(p) {
            return (rows, y);
        }
        let eta = self.plan_etas(p, plan, running, passed);
        let cursor = running.and_then(|id| plan.index_of(id));
        let sel = self.selected_cp();
        for (i, st) in plan.steps.iter().enumerate().take(10) {
            let gone = passed.contains(&st.id) && cursor.is_none_or(|k| i < k);
            let mut what = Vec::new();
            if !st.actions.is_empty() {
                what.push(self.actions_text(&st.actions));
            }
            if let Some(wt) = &st.wait {
                what.push(format!("WAIT {}", self.condition_set_text(wt)));
            }
            let label = format!("CP {:<2} {}", i + 1, eta.get(i).map_or("", |s| s.as_str()));
            let colour = if gone {
                style::TEXT_OFF
            } else if cursor == Some(i) && look == Look::Running {
                style::AMBER
            } else {
                style::TEXT
            };
            rows.push(
                Button::new(
                    Rect::new(x + 6.0, y, w - 12.0, 20.0),
                    hud::Look::Row,
                    label,
                    GameAction::PlanSelect(st.id),
                )
                .value(what.join(" · "))
                .colour(colour)
                .active(sel == Some(st.id))
                .tip("SELECT THIS CHECKPOINT (EDITS THE PLAN)"),
            );
            y += 22.0;
        }
        let Some(st) = sel.and_then(|id| plan.index_of(id)).map(|i| &plan.steps[i]) else { return (rows, y) };
        let leg = &st.leg;
        let keep = |s: Option<String>| s.unwrap_or_else(|| "KEEP".into());
        let values = [
            keep(leg.speed.map(speed_text)),
            keep(leg.formation.map(format_formation)),
            keep(leg.technique.map(|t| short_technique(t).into())),
            keep(leg.weapons.map(|w| weapons_str(w).into())),
            keep(leg.on_contact.map(|r| reaction_short(r).into())),
            self.actions_text(&st.actions),
            st.wait.as_ref().map_or("NONE".into(), |w| self.condition_set_text(w)),
            self.actions_text(&st.on_release),
            st.branch.as_ref().map_or("NONE".into(), |b| self.branch_text(plan, b)),
        ];
        for (k, v) in values.into_iter().enumerate() {
            let label = format!("{} {}", if k < 5 { "  LEG" } else { "  CP " }, ROW_NAMES[k]);
            rows.push(
                Button::new(Rect::new(x + 6.0, y, w - 12.0, 20.0), hud::Look::Row, label, GameAction::PlanRow(k as u8))
                    .value(v)
                    .colour(style::TEXT_DIM)
                    .tip("CHANGE (OPENS THE CHECKPOINT MENU HERE)"),
            );
            y += 22.0;
        }
        (rows, y)
    }
}

/// Leg settings as a short label ("25 KM/H · WEDGE · TRAV OVW"), "KEEP" if none is set.
fn leg_text(l: &sim::view::LegSettings) -> String {
    let mut v: Vec<String> = Vec::new();
    if let Some(s) = l.speed {
        v.push(speed_text(s));
    }
    if let Some(f) = l.formation {
        v.push(format_formation(f));
    }
    if let Some(t) = l.technique {
        v.push(short_technique(t).into());
    }
    if let Some(w) = l.weapons {
        v.push(format!("WPN {}", weapons_str(w)));
    }
    if let Some(r) = l.on_contact {
        v.push(format!("CONTACT {}", reaction_short(r)));
    }
    if let Some(r) = l.on_ambush {
        v.push(format!("AMBUSH {}", ambush_short(r)));
    }
    if let Some(b) = l.built_up {
        v.push(format!("BUILT-UP {}", built_up_name(b)));
    }
    if v.is_empty() { "KEEP".into() } else { v.join(" · ") }
}

/// One drawn leg: from `a` to step `idx` (id `id`).
struct Leg {
    idx: usize,
    id: u16,
    a: [f64; 2],
    b: [f64; 2],
    passed: bool,
}

/// Legs of a plan: passed ones from checkpoint to checkpoint, the current one from the
/// platoon, then the rest.
fn plan_legs(plan: &Plan, running: Option<u16>, passed: &[u16], from: Option<[f64; 2]>) -> Vec<Leg> {
    let cursor = running.and_then(|id| plan.index_of(id)).unwrap_or(0);
    let mut out = Vec::new();
    let mut prev: Option<[f64; 2]> = None;
    for (i, s) in plan.steps.iter().enumerate() {
        let gone = i < cursor && passed.contains(&s.id);
        let a = if i == cursor { from.or(prev) } else { prev };
        if let Some(a) = a {
            out.push(Leg { idx: i, id: s.id, a, b: s.at, passed: gone });
        }
        prev = Some(s.at);
    }
    out
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use sim_data::{MapRef, SideDef, UnitDef};
    use terrain::{AuthoringPackage, MapManifest, bake};

    use super::posture::Posture;
    use super::*;

    fn unit(cs: &str, x: f64, platoon: &str) -> UnitDef {
        UnitDef {
            callsign: cs.into(),
            vehicle: "bulwark_a1".into(),
            position: [x, 400.0],
            heading_deg: 0.0,
            platoon: Some(platoon.into()),
            squad: None,
            mounted_in: None,
        }
    }

    /// Two own platoons on a flat synthetic map, the planning phase over.
    fn game() -> Game {
        game_with(vec![unit("1-1", 700.0, "1"), unit("2-1", 900.0, "2")], |_| {})
    }

    /// Blue `units` on a flat synthetic map edited by `edit`, the planning phase over.
    fn game_with(blue: Vec<UnitDef>, edit: impl FnOnce(&mut AuthoringPackage)) -> Game {
        let ds = Arc::new(DataSet::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")).unwrap());
        let manifest = MapManifest {
            id: "synthetic".into(),
            name: "synthetic".into(),
            width_cells: 400,
            height_cells: 400,
            cell_size_m: 4.0,
            seed: 1,
            recipe: None,
        };
        let mut pkg = AuthoringPackage::new_flat(manifest, 100.0);
        pkg.landcover.data.fill(ds.terrain.landcover_id("cropland").unwrap());
        edit(&mut pkg);
        let classes = terrain::mobility::all_classes(&ds);
        let map = Arc::new(bake(&pkg, &ds.terrain, &classes));
        let side = |name: &str, doctrine: &str, units: Vec<UnitDef>| SideDef {
            name: name.into(),
            doctrine: doctrine.into(),
            tech_year: None,
            units,
            ai: None,
            fire_support: vec![],
            supply_points: vec![],
        };
        let sc = ScenarioDef {
            id: "t".into(),
            name: "t".into(),
            date: (1990, 6, 1),
            start_time_s: 10 * 3600,
            map: MapRef { id: "synthetic".into(), recipe: "mixed-test".into(), seed: 1, size_m: 0.0 },
            seed: 7,
            sides: vec![
                side("BLUE", "western-combined", blue),
                side("RED", "generic-opfor", vec![UnitDef { position: [800.0, 1500.0], ..unit("211", 0.0, "21") }]),
            ],
            objectives: vec![],
            victory: None,
            weather: Default::default(),
            briefing: None,
            logistics: vec![],
        };
        let mut g = Game::new(sc, ds, map).unwrap();
        g.execute();
        g
    }

    fn platoons(g: &Game) -> (PlatoonId, PlatoonId) {
        let ids: Vec<PlatoonId> = g.picture().platoons.iter().map(|p| p.id).collect();
        (ids[0], ids[1])
    }

    fn select(g: &mut Game, p: Option<PlatoonId>) {
        g.sel_platoon = p;
        g.sync_drafts();
    }

    #[test]
    fn troop_actions_in_the_cp_menu_follow_the_platoon() {
        // An M2 carrying a squad (platoons "1" and "1-D") and a house at (700, 1000).
        let carrier = UnitDef { vehicle: "ward_m2".into(), ..unit("1-1", 700.0, "1") };
        let squad = UnitDef {
            vehicle: String::new(),
            squad: Some("rifle_squad_b".into()),
            mounted_in: Some("1-1".into()),
            platoon: None,
            ..unit("1-1A", 0.0, "")
        };
        let mut g = game_with(vec![carrier, squad], |p| {
            p.features.buildings.push(terrain::features::Building {
                id: 1,
                ring: vec![[690.0, 990.0], [710.0, 990.0], [710.0, 1005.0], [690.0, 1005.0]],
                props: terrain::features::BuildingProps { height_m: 6.0, class: sim_data::BuildingClass::Masonry },
            });
        });
        let id = |g: &Game, cs: &str| g.picture().platoons.iter().find(|p| p.callsign == cs).unwrap().id;
        let at_cp = |g: &mut Game, p: PlatoonId, cp: [f64; 2]| -> Vec<(String, bool)> {
            select(g, Some(p));
            g.plan_add_cp(cp);
            g.sync_drafts();
            let sid = g.plan.as_ref().unwrap().plan.steps.last().unwrap().id;
            let (_, items) = g.plan_cp_menu(sid);
            let acts = items.into_iter().find(|i| i.label == "AT CP").unwrap().sub;
            acts.into_iter().map(|i| (i.label, i.enabled)).collect()
        };
        let (p1, p1d) = (id(&g, "1"), id(&g, "1-D"));
        let carriers = at_cp(&mut g, p1, [700.0, 800.0]);
        assert!(carriers.contains(&("DISMOUNT".into(), true)) && carriers.contains(&("MOUNT".into(), true)));
        let squads = at_cp(&mut g, p1d, [700.0, 997.0]);
        assert!(squads.contains(&("OCCUPY BUILDING".into(), true)), "{squads:?}");
        assert!(!squads.iter().any(|(l, _)| l == "FACE..." || l == "SMOKE SCREEN" || l == "DISMOUNT"), "{squads:?}");
        let open = at_cp(&mut g, p1d, [700.0, 1300.0]);
        assert!(open.contains(&("OCCUPY BUILDING".into(), false)), "no house near the CP: {open:?}");
    }

    #[test]
    fn draft_survives_selection_change_and_esc() {
        let mut g = game();
        let (a, b) = platoons(&g);
        select(&mut g, Some(a));
        g.plan_add_cp([700.0, 800.0]);
        g.plan_add_cp([700.0, 1000.0]);
        g.sync_drafts();
        assert_eq!(g.plan.as_ref().map(|d| d.plan.steps.len()), Some(2));

        select(&mut g, Some(b));
        assert!(g.plan.is_none() && g.has_draft(a), "the draft waits, it is not reverted");
        select(&mut g, Some(a));
        assert_eq!(g.plan.as_ref().map(|d| d.plan.steps.len()), Some(2));

        // Esc steps back (here: deselects) and never discards the draft.
        g.press(Cmd::Cancel);
        g.sync_drafts();
        assert!(g.sel_platoon.is_none());
        assert!(g.has_draft(a));
        select(&mut g, Some(a));
        assert_eq!(g.plan.as_ref().map(|d| d.plan.steps.len()), Some(2));
    }

    #[test]
    fn undo_redo_and_confirmed_revert() {
        let mut g = game();
        let (a, _) = platoons(&g);
        select(&mut g, Some(a));
        g.plan_add_cp([700.0, 800.0]);
        g.sync_drafts();
        g.plan_add_cp([700.0, 1000.0]);
        g.sync_drafts();
        g.press(Cmd::Undo);
        assert_eq!(g.plan.as_ref().map(|d| d.plan.steps.len()), Some(1));
        g.press(Cmd::Redo);
        assert_eq!(g.plan.as_ref().map(|d| d.plan.steps.len()), Some(2));

        g.press(Cmd::PlanRevert);
        assert!(g.plan.is_some() && g.revert_armed, "two edits: the first REVERT only asks");
        g.press(Cmd::PlanRevert);
        assert!(g.plan.is_none());
    }

    #[test]
    fn selecting_a_checkpoint_starts_no_draft() {
        let mut g = game();
        let (a, _) = platoons(&g);
        select(&mut g, Some(a));
        g.select_cp(1);
        assert!(g.plan.is_none());
        assert_eq!(g.selected_cp(), Some(1));
    }

    #[test]
    fn quick_order_keeps_the_replaced_draft_for_ctrl_z() {
        let mut g = game();
        let (a, _) = platoons(&g);
        select(&mut g, Some(a));
        g.plan_add_cp([700.0, 800.0]);
        g.plan_add_cp([700.0, 1000.0]);
        g.sync_drafts();
        g.issue_move(Vec::new(), [1200.0, 1200.0]);
        assert!(g.plan.is_none(), "the move replaced the draft");
        g.press(Cmd::Undo);
        assert_eq!(g.plan.as_ref().map(|d| d.plan.steps.len()), Some(2), "Ctrl+Z restores it as a draft");
    }

    #[test]
    fn a_posture_pick_with_no_cp_selected_orders_the_platoon_now() {
        let mut g = game();
        let (a, _) = platoons(&g);
        select(&mut g, Some(a));
        let scope = g.posture_scope(Posture::Formation).unwrap();
        assert_eq!(scope, posture::Scope::Now(Addressee::Platoon(a)));
        g.apply_posture(scope, posture::PostureVal::Formation(Formation::Line));
        assert_eq!(g.shown_formation(a), Some(Formation::Line));
        let scope = g.posture_scope(Posture::Technique).unwrap();
        g.apply_posture(scope, posture::PostureVal::Technique(Technique::BoundingOverwatch));
        assert_eq!(g.shown_technique(a), Some(Technique::BoundingOverwatch));
        assert_eq!(g.posture_value(Posture::Technique), "BOUND OVW");
        g.advance_ticks(2000);
        assert_eq!(g.picture().platoon(a).unwrap().technique, Technique::BoundingOverwatch);
    }

    #[test]
    fn a_posture_pick_with_a_cp_selected_edits_that_leg_only() {
        let mut g = game();
        let (a, _) = platoons(&g);
        select(&mut g, Some(a));
        g.plan_add_cp([700.0, 800.0]);
        g.plan_add_cp([700.0, 1000.0]);
        g.sync_drafts();
        let (first, second) = {
            let d = g.plan.as_ref().unwrap();
            (d.plan.steps[0].id, d.plan.steps[1].id)
        };
        g.select_cp(first);
        let scope = g.posture_scope(Posture::Speed).unwrap();
        assert_eq!(scope, posture::Scope::Leg(a, first));
        g.apply_posture(scope, posture::PostureVal::Speed(Some(8.0)));
        g.apply_posture(scope, posture::PostureVal::Formation(Formation::Vee));
        let d = g.plan.as_ref().unwrap();
        assert_eq!(d.plan.steps[0].leg.speed, Some(SpeedSetting::Limit(8.0)));
        assert_eq!(d.plan.steps[0].leg.formation, Some(Formation::Vee));
        assert_eq!(d.plan.steps[1].leg.speed, None);
        assert!(g.pending_formation.is_empty(), "nothing was ordered now");
        assert_eq!(g.posture_value(Posture::Speed), "29 KM/H");

        g.apply_posture(scope, posture::PostureVal::Keep(Posture::Speed));
        assert_eq!(g.plan.as_ref().unwrap().plan.steps[0].leg.speed, None);
        assert_eq!(g.posture_value(Posture::Speed), "KEEP");

        g.select_cp(second);
        assert_eq!(g.posture_scope(Posture::Speed), Some(posture::Scope::Leg(a, second)));
        assert_eq!(
            g.posture_scope(Posture::Ammo),
            Some(posture::Scope::Now(Addressee::Platoon(a))),
            "ammo is never a leg setting"
        );
    }

    fn route_len(g: &Game, p: PlatoonId) -> Option<usize> {
        g.graphics.iter().find_map(|o| match &o.kind {
            GraphicKind::Route { points } if o.platoon == Some(p) => Some(points.len()),
            _ => None,
        })
    }

    #[test]
    fn ctrl_arrows_move_a_cursor_and_ctrl_space_places_a_point() {
        let mut g = game();
        let (a, _) = platoons(&g);
        select(&mut g, Some(a));
        g.tool = Tool::Move;
        let cam = Camera { centre: [800.0, 800.0], mpp: 2.0, screen: [1600.0, 900.0] };
        let mut inp = Input { ctrl: true, screen: [1600.0, 900.0], ..Input::default() };
        inp.pressed = vec![Key::Right];
        let (k, _) = g.keyboard_click(&inp, &cam).expect("cursor moves");
        assert!(!k.left_pressed);
        let r = g.reticle.unwrap();
        inp.pressed = vec![Key::Space];
        let (k, _) = g.keyboard_click(&inp, &cam).expect("places");
        assert!(k.left_pressed && !k.ctrl && !k.hit(Key::Space));
        let w = cam.to_world(k.mouse);
        assert!((w[0] - r[0]).abs() < 1.0 && (w[1] - r[1]).abs() < 1.0);
    }

    #[test]
    fn followers_get_their_own_route_graphic() {
        let mut g = game();
        let (a, b) = platoons(&g);
        select(&mut g, Some(a));
        g.also_of = Some(a);
        g.add_follower(b);
        g.issue_move(vec![], [700.0, 900.0]);
        for p in [a, b] {
            assert!(g.graphics.iter().any(|o| o.platoon == Some(p) && matches!(o.kind, GraphicKind::Route { .. })));
        }
    }

    #[test]
    fn shift_right_click_appends_and_sends_a_plain_move_on_release() {
        let mut g = game();
        let (a, _) = platoons(&g);
        select(&mut g, Some(a));
        g.shift_held = true;
        g.shift_append([700.0, 800.0]);
        g.shift_append([700.0, 1000.0]);
        g.sync_drafts();
        assert_eq!(g.plan.as_ref().map(|d| d.plan.steps.len()), Some(2));
        assert_eq!(route_len(&g, a), None, "nothing sent while Shift is down");
        g.shift_held = false;
        g.flush_quasi();
        assert!(g.plan.is_none());
        assert_eq!(route_len(&g, a), Some(3), "from the platoon, via CP 1, to CP 2");
        assert!(!g.graphics.iter().any(|o| matches!(o.kind, GraphicKind::Plan { .. })));
        g.advance_ticks(2000);
        assert!(g.picture().platoon(a).unwrap().plan.is_none(), "a plain move, not a plan");
    }

    #[test]
    fn shift_right_click_on_a_running_plan_sends_a_plan_and_ctrl_z_restores_the_old_one() {
        let mut g = game();
        let (a, _) = platoons(&g);
        select(&mut g, Some(a));
        g.plan_add_cp([700.0, 800.0]);
        g.plan_add_cp([700.0, 1000.0]);
        g.plan.as_mut().unwrap().plan.steps[0].wait = Some(one(TriggerCondition::Elapsed { s: 3000.0 }));
        g.send_plan();
        g.advance_ticks(2000);
        select(&mut g, Some(a));
        assert!(g.plan.is_none());

        g.shift_held = true;
        g.shift_append([700.0, 1200.0]);
        assert_eq!(g.plan.as_ref().map(|d| d.plan.steps.len()), Some(3), "appended to the running plan");
        g.shift_held = false;
        g.flush_quasi();
        g.advance_ticks(2000);
        let v = g.picture().platoon(a).unwrap().plan.as_ref().unwrap().plan.steps.len();
        assert_eq!(v, 3);

        g.press(Cmd::Undo);
        assert_eq!(g.plan.as_ref().map(|d| d.plan.steps.len()), Some(2), "Ctrl+Z brings the old plan back as a draft");
    }

    #[test]
    fn an_explicit_draft_is_not_sent_when_shift_is_released() {
        let mut g = game();
        let (a, _) = platoons(&g);
        select(&mut g, Some(a));
        g.plan_add_cp([700.0, 800.0]);
        g.sync_drafts();
        g.shift_held = true;
        g.shift_append([700.0, 1000.0]);
        g.shift_held = false;
        g.flush_quasi();
        assert_eq!(g.plan.as_ref().map(|d| d.plan.steps.len()), Some(2));
        assert_eq!(route_len(&g, a), None);
    }

    #[test]
    fn shift_battle_position_and_smoke_append_actions() {
        let mut g = game();
        let (a, _) = platoons(&g);
        select(&mut g, Some(a));
        g.shift_held = true;
        g.append_tool = true;
        g.issue_bp([700.0, 1000.0], 0.5);
        assert!(g.append_smoke());
        let d = g.plan.as_ref().unwrap();
        assert_eq!(d.plan.steps.len(), 1);
        assert!(matches!(d.plan.steps[0].actions[0], StepAction::OccupyBp { .. }));
        assert!(matches!(d.plan.steps[0].actions[1], StepAction::SmokeScreen));
        g.shift_held = false;
        g.flush_quasi();
        assert!(g.plan.is_none());
        g.advance_ticks(2000);
        assert!(g.picture().platoon(a).unwrap().plan.is_some());
    }

    #[test]
    fn shift_smoke_without_a_checkpoint_adds_no_draft() {
        let mut g = game();
        let (a, _) = platoons(&g);
        select(&mut g, Some(a));
        assert!(!g.append_smoke());
        assert!(g.plan.is_none() && !g.has_draft(a));
    }

    #[test]
    fn esc_cancels_a_pending_click_before_the_tool() {
        let mut g = game();
        let (a, _) = platoons(&g);
        select(&mut g, Some(a));
        g.plan_add_cp([700.0, 800.0]);
        g.sync_drafts();
        g.tool = Tool::BattlePosition { centre: Some([700.0, 900.0]) };
        g.press(Cmd::Cancel);
        assert_eq!(g.tool, Tool::BattlePosition { centre: None });
        g.press(Cmd::Cancel);
        assert_eq!(g.tool, Tool::None);
        assert!(g.plan.is_some() && g.sel_platoon == Some(a));
    }

    #[test]
    fn every_card_cell_has_its_own_hotkey() {
        let mut g = game();
        let (a, _) = platoons(&g);
        select(&mut g, Some(a));
        let cells = g.card();
        let mut keys: Vec<&str> = cells.iter().map(|(_, b)| b.key).collect();
        assert!(keys.iter().all(|k| !k.is_empty()), "a card cell has no hotkey");
        let mut at: Vec<usize> = cells.iter().map(|(i, _)| *i).collect();
        keys.sort_unstable();
        at.sort_unstable();
        assert!(keys.windows(2).all(|w| w[0] != w[1]), "duplicate hotkeys: {keys:?}");
        assert!(at.windows(2).all(|w| w[0] != w[1]), "two buttons share a cell");
        assert!(at.iter().all(|i| *i < hud::CARD_COLS * 3));
        assert!(cells.iter().all(|(_, b)| !b.tip.is_empty()));
    }

    #[test]
    fn on_contact_now_orders_the_platoon_and_on_a_leg_edits_the_plan() {
        let mut g = game();
        let (a, _) = platoons(&g);
        select(&mut g, Some(a));
        let scope = g.posture_scope(Posture::OnContact).unwrap();
        assert_eq!(scope, posture::Scope::Now(Addressee::Platoon(a)));
        g.apply_posture(scope, posture::PostureVal::OnContact(ContactReaction::Continue));
        g.advance_ticks(2000);
        assert_eq!(g.picture().platoon(a).unwrap().contact_reaction, ContactReaction::Continue);
        assert_eq!(g.posture_value(Posture::OnContact), "CONTINUE");

        g.plan_add_cp([700.0, 800.0]);
        g.sync_drafts();
        let id = g.plan.as_ref().unwrap().plan.steps[0].id;
        g.select_cp(id);
        let scope = g.posture_scope(Posture::OnContact).unwrap();
        assert_eq!(scope, posture::Scope::Leg(a, id));
        g.apply_posture(scope, posture::PostureVal::OnContact(ContactReaction::HastyPosition));
        let leg = &g.plan.as_ref().unwrap().plan.steps[0].leg;
        assert_eq!(leg.on_contact, Some(ContactReaction::HastyPosition));
    }

    #[test]
    fn g_opens_the_graphics_list_and_a_pick_arms_the_tool() {
        let mut g = game();
        g.press(Cmd::Graphics);
        assert!(g.graphics_request);
        g.press(Cmd::Trp);
        assert!(matches!(g.tool, Tool::Graphic { kind: DrawKind::Trp, .. }));
    }

    /// TODO J5: a graphic is moved, resized and turned from its menu (same id), and a
    /// contingency re-timed and cancelled from its panel row.
    #[test]
    fn graphics_and_contingencies_edit_after_placement() {
        use super::gfxedit::GfxEdit;
        let mut g = game();
        let bp = SimGraphic::BattlePosition { centre: [800.0, 800.0], radius: 200.0, facing: 0.0 };
        g.add_graphic("BP 1".into(), bp);
        let id = g.picture().graphics[0].id;
        g.run_menu_act(MenuAct::Gfx(GfxEdit::Move(id)), [0.0, 0.0], &mut GameFx::default());
        assert_eq!(g.tool, Tool::MoveGraphic { id });
        g.place_moved_graphic(id, [900.0, 1000.0]);
        g.run_gfx_edit(GfxEdit::Radius(id, 1));
        g.run_gfx_edit(GfxEdit::Turn(id));
        let gr = g.picture().graphic(id).expect("same id");
        let SimGraphic::BattlePosition { centre, radius, facing } = gr.kind else { panic!("still a BP") };
        assert_eq!((centre, radius), ([900.0, 1000.0], 300.0));
        assert!((facing - std::f64::consts::FRAC_PI_4).abs() < 1e-12);

        g.order_mode = OrderMode::InFiveMinutes;
        g.order_one(Command::Halt { to: Addressee::Platoon(PlatoonId(1)) });
        let t = g.picture().triggers[0].id;
        let mut fx = GameFx::default();
        g.act(GameAction::Contingency(t), &mut fx);
        assert_eq!(g.menu_request.as_ref().map(|r| r.0), Some(MenuTarget::Trigger(t)));
        g.run_gfx_edit(GfxEdit::Retime(t, 1));
        assert!(matches!(g.picture().triggers[0].condition, TriggerCondition::AtTime { time_s } if time_s == 600.0));
        g.run_gfx_edit(GfxEdit::CancelTrigger(t));
        assert!(g.picture().triggers.is_empty());
    }
}
