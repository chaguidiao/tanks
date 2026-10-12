//! Game mode: runs the simulation, shows the side's perceived picture, issues orders.
//!
//! Only `PerceivedWorld` is read for display, selection and orders (CLAUDE.md rule 3).
//! Ground truth is drawn only with the `debug-truth` feature.

use std::collections::VecDeque;
use std::sync::Arc;

use sim::view::{
    Condition, ContactLevel, Graphic, GraphicKind as SimGraphic, GunActivity, OwnUnit, PerceivedWorld, PlatoonId, Task,
    TriggerCondition, WeaponsStatus,
};
use sim::{Addressee, Command, EndReason, EntityId, Event, KillKind, PlatoonReport, PositionKind, SideId, Sim};
use sim_data::{DataSet, Formation, ScenarioDef, Technique};
use terrain::BakedMap;
use terrain::ground::Ground;

use crate::aar::{AarView, Recorder};
use crate::format;
use crate::hud::{self, Button, Look, Rect, TOP_H};
use crate::render::{Frame, Input, Key, P2, Rgba};
use crate::style;
use crate::symbols;
use crate::view::{Camera, Minimap};

/// Time compression steps (DESIGN.md §2.1).
pub const SPEEDS: [u32; 4] = [1, 2, 4, 8];
const MAX_TICKS_PER_FRAME: u32 = 40;
/// Zoomed out beyond this (m/px), platoons are drawn as one symbol (DESIGN.md §10.1).
const AGGREGATE_MPP: f64 = 7.0;

#[derive(Debug, Clone, PartialEq)]
pub enum Tool {
    None,
    /// Move: the next left click is the destination.
    Move,
    /// Route with checkpoints: left clicks add checkpoints (in `Game::draft`), right click or
    /// Enter adds the destination and sends.
    Route,
    /// Engage: the next left click picks a contact.
    Engage,
    /// Battle position: centre chosen, waiting for the threat direction click.
    BattlePosition {
        centre: Option<[f64; 2]>,
    },
    Viewshed,
    Los {
        a: Option<[f64; 2]>,
        b: Option<[f64; 2]>,
        result: Option<(bool, f64)>,
    },
    /// Draw a control measure.
    Graphic {
        kind: DrawKind,
        points: Vec<[f64; 2]>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawKind {
    PhaseLine,
    Objective,
    Trp,
}

/// How the next order is issued (DESIGN.md §8.2 contingency orders).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderMode {
    Now,
    /// Execute when an enemy is reported across the most recent phase line.
    OnEnemyCrossing,
    /// Execute five minutes from now.
    InFiveMinutes,
}

/// Commands on the command card; each also has a hotkey.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmd {
    Move,
    Route,
    Halt,
    BattlePosition,
    Engage,
    Formation,
    Technique,
    Speed,
    Weapons,
    Contingency,
    NextPlatoon,
    PhaseLine,
    Objective,
    Trp,
    Viewshed,
    Los,
    /// Finish the route or phase line being drawn.
    Send,
    UndoPoint,
    Cancel,
    RadiusDown,
    RadiusUp,
}

/// What a HUD click or hotkey asks the game to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameAction {
    Pause,
    Speed(usize),
    Execute,
    Aar,
    SelectPlatoon(PlatoonId),
    SelectUnit(EntityId),
    /// DATABASE on the selected vehicle's type.
    VehicleData,
    Cmd(Cmd),
}

const PL_NAMES: [&str; 8] = ["RED", "BLUE", "GREEN", "AMBER", "BLACK", "WHITE", "GOLD", "SILVER"];

/// A graphic the player drew with an order (dashed until acknowledged).
#[derive(Debug, Clone)]
pub struct OrderGraphic {
    pub platoon: Option<PlatoonId>,
    pub unit: Option<EntityId>,
    pub kind: GraphicKind,
    pub acknowledged: bool,
}

#[derive(Debug, Clone)]
pub enum GraphicKind {
    Bp {
        centre: [f64; 2],
        radius: f64,
        facing: f64,
        label: String,
    },
    /// Route sent but not yet received: start, checkpoints, destination.
    Route {
        points: Vec<[f64; 2]>,
    },
}

/// Side effects for the app (textures, camera etc.).
#[derive(Default)]
pub struct GameFx {
    /// Viewshed overlay: (rgba, w, h, origin x, origin y, step m).
    pub viewshed: Option<(Vec<u8>, usize, usize, f64, f64, f64)>,
    pub clear_overlay: bool,
    /// Centre the camera here.
    pub center_on: Option<[f64; 2]>,
    /// Open the DATABASE on this vehicle type.
    pub vehicle_data: Option<String>,
}

pub struct Game {
    pub sim: Sim,
    pub scenario: ScenarioDef,
    pub side: SideId,
    pub sel_platoon: Option<PlatoonId>,
    pub sel_unit: Option<EntityId>,
    pub paused: bool,
    pub speed: usize,
    pub technique: Technique,
    /// March speed for the next move (m/s); None = MAX (formation speed).
    pub move_speed: Option<f64>,
    pub tool: Tool,
    pub bp_radius: f64,
    pub graphics: Vec<OrderGraphic>,
    /// Route being drafted: checkpoints (and, once sent, the destination). Built with
    /// Shift+right click (sent when Shift is released) or with the ROUTE tool.
    pub draft: Vec<[f64; 2]>,
    acc: f64,
    prev: Vec<(EntityId, [f64; 2], f64)>,
    pub log: VecDeque<(String, String)>,
    bp_count: u32,
    pub ended: Option<String>,
    /// Planning phase: the clock is stopped and orders take effect at H-hour without delay.
    pub planning: bool,
    pub order_mode: OrderMode,
    pub recorder: Recorder,
    pub aar: Option<AarView>,
    counts: [u32; 3],
    /// Formation / weapons changes sent but not yet in the picture (orders wait for the next
    /// step, or for the radio): the card shows and cycles from these. (target, value, sent at)
    pending_formation: Vec<(PlatoonId, Formation, f64)>,
    pending_weapons: Vec<(Addressee, WeaponsStatus, f64)>,
    /// HALT (true) / RESUME (false) sent but not yet received.
    pending_held: Vec<(Addressee, bool, f64)>,
}

impl Game {
    pub fn new(scenario: ScenarioDef, ds: Arc<DataSet>, map: Arc<BakedMap>) -> Result<Self, String> {
        let mut sim = Sim::new(&scenario, ds, map)?;
        // The player commands side 0; its commander AI (if the scenario has one) stays off.
        sim.set_ai_enabled(SideId(0), false);
        let mut g = Self {
            sim,
            scenario,
            side: SideId(0),
            sel_platoon: None,
            sel_unit: None,
            paused: true,
            speed: 0,
            technique: Technique::Traveling,
            move_speed: None,
            tool: Tool::None,
            bp_radius: 250.0,
            graphics: Vec::new(),
            draft: Vec::new(),
            acc: 0.0,
            prev: Vec::new(),
            log: VecDeque::new(),
            bp_count: 0,
            ended: None,
            planning: true,
            order_mode: OrderMode::Now,
            recorder: Recorder::default(),
            aar: None,
            counts: [0; 3],
            pending_formation: Vec::new(),
            pending_weapons: Vec::new(),
            pending_held: Vec::new(),
        };
        if let Some((id, p)) = g.picture().own.first().map(|u| (u.id, u.platoon)) {
            g.sel_unit = Some(id);
            g.sel_platoon = p;
        }
        g.capture_prev();
        let name = g.scenario.name.clone();
        g.radio(format!(
            "{name}. PLANNING: ISSUE ORDERS, DRAW GRAPHICS, THEN EXECUTE (ENTER). HOVER A BUTTON FOR HELP, F7 KEYS."
        ));
        Ok(g)
    }

    /// Esc would cancel a tool or a drafted route (otherwise the app opens its menu).
    pub fn esc_in_use(&self) -> bool {
        self.tool != Tool::None || !self.draft.is_empty()
    }

    pub fn picture(&self) -> &PerceivedWorld {
        self.sim.perceived(self.side)
    }

    pub fn radio(&mut self, text: String) {
        self.log.push_back((format::clock(self.sim.clock_s()), text));
        while self.log.len() > 300 {
            self.log.pop_front();
        }
    }

    fn capture_prev(&mut self) {
        self.prev = self.picture().own.iter().map(|u| (u.id, u.pos, u.heading)).collect();
    }

    fn addressee(&self) -> Option<Addressee> {
        match (self.sel_platoon, self.sel_unit) {
            (Some(p), _) => Some(Addressee::Platoon(p)),
            (None, Some(u)) => Some(Addressee::Unit(u)),
            _ => None,
        }
    }

    /// Callsign used in radio text for a platoon.
    fn platoon_cs(&self, p: PlatoonId) -> String {
        self.picture().platoon(p).map_or("?".into(), |v| v.callsign.clone())
    }

    pub fn order_move(&mut self, unit: EntityId, dest: [f64; 2]) {
        self.order(Command::Move { unit, destination: dest, via: Vec::new(), speed_limit_mps: None });
    }

    /// Centre of the selection's operational vehicles (platoon or single unit).
    fn selection_centre(&self) -> Option<[f64; 2]> {
        let pic = self.picture();
        let ids: Vec<EntityId> = match (self.sel_platoon, self.sel_unit) {
            (Some(p), _) => pic.platoon(p)?.members.clone(),
            (None, Some(u)) => vec![u],
            _ => return None,
        };
        let ps: Vec<[f64; 2]> = ids
            .iter()
            .filter_map(|m| pic.unit(*m))
            .filter(|u| u.condition != Condition::Destroyed)
            .map(|u| u.pos)
            .collect();
        if ps.is_empty() {
            return None;
        }
        let n = ps.len() as f64;
        Some([ps.iter().map(|p| p[0]).sum::<f64>() / n, ps.iter().map(|p| p[1]).sum::<f64>() / n])
    }

    /// Move the selection to `dest` through checkpoints `via` (route with checkpoints).
    pub fn issue_move(&mut self, via: Vec<[f64; 2]>, dest: [f64; 2]) {
        let start = self.selection_centre();
        let cps: Vec<String> =
            via.iter().enumerate().map(|(i, p)| format!("CP {} GRID {}", i + 1, format::grid(*p))).collect();
        let route_txt = if cps.is_empty() { String::new() } else { format!(" VIA {},", cps.join(", ")) };
        let mut points: Vec<[f64; 2]> = start.into_iter().collect();
        points.extend(via.iter().copied());
        points.push(dest);
        match (self.sel_platoon, self.sel_unit) {
            (Some(p), _) => {
                let technique = self.technique;
                let speed_limit_mps = self.move_speed;
                self.graphics.retain(|g| g.platoon != Some(p));
                self.graphics.push(OrderGraphic {
                    platoon: Some(p),
                    unit: None,
                    kind: GraphicKind::Route { points },
                    acknowledged: false,
                });
                let cs = self.platoon_cs(p);
                self.radio(format!(
                    "TO {cs}: MOVE{route_txt} TO GRID {}, {}{}.",
                    format::grid(dest),
                    format_technique(technique),
                    speed_radio(speed_limit_mps)
                ));
                self.order(Command::PlatoonMove {
                    platoon: p,
                    destination: dest,
                    via,
                    formation: None,
                    technique,
                    speed_limit_mps,
                });
            }
            (None, Some(u)) => {
                self.graphics.retain(|g| g.unit != Some(u));
                self.graphics.push(OrderGraphic {
                    platoon: None,
                    unit: Some(u),
                    kind: GraphicKind::Route { points },
                    acknowledged: false,
                });
                if !cps.is_empty() || self.move_speed.is_some() {
                    let cs = self.picture().unit(u).map_or("?".into(), |x| x.callsign.clone());
                    let sp = speed_radio(self.move_speed);
                    self.radio(format!("TO {cs}: MOVE{route_txt} TO GRID {}{sp}.", format::grid(dest)));
                }
                self.order(Command::Move { unit: u, destination: dest, via, speed_limit_mps: self.move_speed });
            }
            _ => {}
        }
    }

    /// Send the drafted route: the last point is the destination.
    fn send_route(&mut self) {
        let mut pts = std::mem::take(&mut self.draft);
        if let Some(dest) = pts.pop() {
            self.issue_move(pts, dest);
        }
        if self.tool == Tool::Route {
            self.tool = Tool::None;
        }
    }

    /// Most recently drawn phase line (trigger reference).
    fn last_phase_line(&self) -> Option<&Graphic> {
        self.picture().graphics.iter().rev().find(|g| matches!(g.kind, SimGraphic::PhaseLine { .. }))
    }

    /// Issue an order according to the planning phase and the current order mode.
    pub fn order(&mut self, cmd: Command) {
        if self.planning && self.order_mode == OrderMode::Now {
            self.sim.submit_immediate(self.side, cmd);
            return;
        }
        match self.order_mode {
            OrderMode::Now => self.sim.submit(self.side, cmd),
            OrderMode::OnEnemyCrossing => match self.last_phase_line().map(|g| (g.id, g.label.clone())) {
                Some((line, label)) => {
                    self.sim.submit_immediate(
                        self.side,
                        Command::AddTrigger { condition: TriggerCondition::EnemyCrosses { line }, then: Box::new(cmd) },
                    );
                    self.sim.settle();
                    self.radio(format!("CONTINGENCY SET: ON ENEMY CROSSING {label}."));
                }
                None => {
                    self.radio("NO PHASE LINE DRAWN. ORDER SENT NOW.".into());
                    self.sim.submit(self.side, cmd);
                }
            },
            OrderMode::InFiveMinutes => {
                let t = self.sim.time_s() + 300.0;
                self.sim.submit_immediate(
                    self.side,
                    Command::AddTrigger { condition: TriggerCondition::AtTime { time_s: t }, then: Box::new(cmd) },
                );
                self.sim.settle();
                self.radio(format!("CONTINGENCY SET: AT {}.", format::clock(self.sim.clock_s() + 300.0)));
            }
        }
        self.order_mode = OrderMode::Now;
    }

    pub fn advance_ticks(&mut self, n: u32) {
        for _ in 0..n {
            self.capture_prev();
            let events = self.sim.step();
            self.recorder.observe(&self.sim, &events);
            for e in events {
                self.on_event(&e);
            }
        }
        self.prune_pending();
    }

    fn on_event(&mut self, e: &Event) {
        match e {
            Event::OrderReceived { side, to: Addressee::Platoon(p) } if *side == self.side => {
                for g in self.graphics.iter_mut().filter(|g| g.platoon == Some(*p)) {
                    g.acknowledged = true;
                }
                // The platoon now shows its own planned route.
                self.graphics.retain(|g| !(g.platoon == Some(*p) && matches!(g.kind, GraphicKind::Route { .. })));
            }
            Event::RoutePlanned { unit, .. } | Event::NoRoute { unit } => {
                self.graphics.retain(|g| g.unit != Some(*unit));
            }
            _ => {}
        }
        if let Some(t) = radio_text(e, self.sim.perceived(self.side), self) {
            if matches!(e, Event::ScenarioEnd { .. }) {
                self.ended = Some(t.clone());
                self.paused = true;
            }
            self.radio(t);
        }
    }

    /// Hotkeys, as the same actions the HUD buttons issue.
    fn hotkeys(&self, inp: &Input) -> Vec<GameAction> {
        use Cmd as C;
        let mut a = Vec::new();
        let drawing_line =
            matches!(&self.tool, Tool::Graphic { kind: DrawKind::PhaseLine, points } if !points.is_empty());
        if inp.hit(Key::Enter) {
            if drawing_line || !self.draft.is_empty() {
                a.push(GameAction::Cmd(C::Send));
            } else if self.planning {
                a.push(GameAction::Execute);
            }
        }
        for (k, c) in [
            (Key::Char('k'), C::Contingency),
            (Key::Char('v'), C::Viewshed),
            (Key::Char('l'), C::Los),
            (Key::Char('b'), C::BattlePosition),
            (Key::LeftBracket, C::RadiusDown),
            (Key::RightBracket, C::RadiusUp),
            (Key::Char('t'), C::Technique),
            (Key::Char('s'), C::Speed),
            (Key::Char('f'), C::Formation),
            (Key::Char('w'), C::Weapons),
            (Key::Char('h'), C::Halt),
            (Key::Char('m'), C::Move),
            (Key::Char('a'), C::Engage),
            (Key::Tab, C::NextPlatoon),
            (Key::Escape, C::Cancel),
            (Key::Backspace, C::UndoPoint),
        ] {
            if inp.hit(k) {
                a.push(GameAction::Cmd(c));
            }
        }
        if inp.hit(Key::Char('r')) && !inp.ctrl {
            a.push(GameAction::Cmd(C::Route));
        }
        if inp.hit(Key::Char('g')) {
            // G cycles PL -> OBJ -> TRP -> off.
            a.push(GameAction::Cmd(match &self.tool {
                Tool::Graphic { kind: DrawKind::PhaseLine, .. } => C::Objective,
                Tool::Graphic { kind: DrawKind::Objective, .. } => C::Trp,
                Tool::Graphic { kind: DrawKind::Trp, .. } => C::Trp,
                _ => C::PhaseLine,
            }));
        }
        if inp.hit(Key::Space) {
            a.push(GameAction::Pause);
        }
        if inp.hit(Key::Char('i')) {
            a.push(GameAction::VehicleData);
        }
        for k in 0..SPEEDS.len() {
            if inp.hit(Key::Num(k as u8 + 1)) {
                a.push(GameAction::Speed(k));
            }
        }
        a
    }

    pub fn update(&mut self, inp: &Input, cam: &Camera, over_ui: bool, clicked: Option<GameAction>) -> GameFx {
        let mut fx = GameFx::default();
        if inp.hit(Key::F(8)) || clicked == Some(GameAction::Aar) {
            self.toggle_aar();
        }
        if let Some(a) = &mut self.aar {
            a.update(inp, &self.recorder);
            if inp.ctrl && inp.hit(Key::Char('r')) {
                let rep = sim::Replay::record(&self.sim);
                let path = format!("out/replay-{}.ron", self.scenario.id);
                let msg = std::fs::create_dir_all("out")
                    .and_then(|_| std::fs::write(&path, rep.to_ron()))
                    .map_or_else(|e| format!("REPLAY SAVE FAILED: {e}"), |_| format!("REPLAY SAVED: {path}"));
                self.radio(msg);
            }
            return fx;
        }
        let mut actions = self.hotkeys(inp);
        actions.extend(clicked.filter(|a| *a != GameAction::Aar));
        for a in actions {
            self.act(a, &mut fx);
        }

        let world = cam.to_world(inp.mouse);
        if !over_ui && inp.left_pressed {
            match self.tool.clone() {
                Tool::None => self.select(cam, inp.mouse, inp.ctrl),
                Tool::Move => {
                    self.issue_move(Vec::new(), world);
                    self.tool = Tool::None;
                }
                Tool::Route => self.draft.push(world),
                Tool::Engage => {
                    self.engage_at(cam, inp.mouse);
                    self.tool = Tool::None;
                }
                Tool::BattlePosition { centre: None } => self.tool = Tool::BattlePosition { centre: Some(world) },
                Tool::BattlePosition { centre: Some(c) } => {
                    let facing = ai::drive::bearing(c, world);
                    self.issue_bp(c, facing);
                    self.tool = Tool::None;
                }
                Tool::Viewshed => fx.viewshed = Some(self.viewshed(world)),
                Tool::Los { a: None, .. } | Tool::Los { b: Some(_), .. } => {
                    self.tool = Tool::Los { a: Some(world), b: None, result: None };
                }
                Tool::Los { a: Some(a), b: None, .. } => {
                    let r = self.los(a, world);
                    self.tool = Tool::Los { a: Some(a), b: Some(world), result: Some(r) };
                }
                Tool::Graphic { kind, mut points } => match kind {
                    DrawKind::PhaseLine => {
                        points.push(world);
                        self.tool = Tool::Graphic { kind, points };
                    }
                    DrawKind::Objective if points.is_empty() => self.tool = Tool::Graphic { kind, points: vec![world] },
                    DrawKind::Objective => {
                        self.counts[1] += 1;
                        let r =
                            ((world[0] - points[0][0]).powi(2) + (world[1] - points[0][1]).powi(2)).sqrt().max(50.0);
                        let label = format!("OBJ {}", (b'A' + ((self.counts[1] - 1) % 26) as u8) as char);
                        self.add_graphic(label, SimGraphic::Objective { centre: points[0], radius: r });
                        self.tool = Tool::Graphic { kind, points: vec![] };
                    }
                    DrawKind::Trp => {
                        self.counts[2] += 1;
                        let label = format!("TRP {}", self.counts[2]);
                        self.add_graphic(label, SimGraphic::Trp { at: world });
                    }
                },
            }
        }
        if !over_ui && inp.right_pressed {
            match &self.tool {
                // Right click finishes a route at the clicked destination.
                Tool::Route => {
                    self.draft.push(world);
                    self.send_route();
                }
                // Right click cancels a targeting mode (RTS convention).
                Tool::Move | Tool::Engage | Tool::BattlePosition { .. } | Tool::Graphic { .. } => {
                    self.tool = Tool::None;
                }
                _ if inp.ctrl => self.engage_at(cam, inp.mouse),
                _ if inp.shift => {
                    // Shift+right click queues checkpoints, appended to the current route.
                    if self.draft.is_empty()
                        && let Some(v) = self.sel_platoon.and_then(|p| self.picture().platoon(p))
                    {
                        self.draft = v.waypoints.clone();
                    }
                    if self.sel_platoon.is_some() || self.sel_unit.is_some() {
                        self.draft.push(world);
                    }
                }
                _ => {
                    self.draft.clear();
                    self.issue_move(Vec::new(), world);
                }
            }
        }
        // Shift released: the queued route goes out as one order.
        if !inp.shift && !self.draft.is_empty() && self.tool != Tool::Route {
            self.send_route();
        }
        if !self.paused && !self.planning {
            self.acc += inp.frame_time as f64 * SPEEDS[self.speed] as f64;
            let n = ((self.acc / sim::DT) as u32).min(MAX_TICKS_PER_FRAME);
            self.acc -= n as f64 * sim::DT;
            self.acc = self.acc.min(sim::DT * 2.0);
            self.advance_ticks(n);
        }
        fx
    }

    fn toggle_aar(&mut self) {
        if self.aar.is_some() {
            self.aar = None;
        } else {
            self.open_aar();
        }
    }

    fn act(&mut self, a: GameAction, fx: &mut GameFx) {
        match a {
            GameAction::Pause => {
                if self.planning {
                    self.radio("PLANNING PHASE: EXECUTE (ENTER) TO START THE CLOCK.".into());
                } else {
                    self.paused = !self.paused;
                }
            }
            GameAction::Speed(k) => {
                self.speed = k.min(SPEEDS.len() - 1);
                if !self.planning && self.ended.is_none() {
                    self.paused = false;
                }
            }
            GameAction::Execute => self.execute(),
            GameAction::Aar => self.toggle_aar(),
            GameAction::SelectPlatoon(p) => {
                if self.sel_platoon == Some(p) {
                    fx.center_on = self.selection_centre();
                } else {
                    self.sel_platoon = Some(p);
                    self.sel_unit = self.picture().platoon(p).and_then(|v| v.members.first().copied());
                    self.draft.clear();
                }
            }
            GameAction::SelectUnit(u) => {
                if self.sel_unit == Some(u) && self.sel_platoon.is_none() {
                    fx.center_on = self.selection_centre();
                } else {
                    self.sel_unit = Some(u);
                    self.sel_platoon = None;
                    self.draft.clear();
                }
            }
            GameAction::VehicleData => {
                fx.vehicle_data = self.sel_unit.and_then(|u| self.picture().unit(u)).map(|u| u.vehicle_id.clone());
            }
            GameAction::Cmd(c) => self.command(c, fx),
        }
    }

    fn toggle_tool(&mut self, t: Tool) {
        self.tool = if std::mem::discriminant(&self.tool) == std::mem::discriminant(&t) { Tool::None } else { t };
    }

    /// Run a command-card action outside the frame loop (scripted checks: `--press`).
    pub fn press(&mut self, c: Cmd) {
        self.command(c, &mut GameFx::default());
    }

    fn command(&mut self, c: Cmd, fx: &mut GameFx) {
        let selected = self.addressee().is_some();
        if self.tool == Tool::Viewshed && !matches!(c, Cmd::Viewshed) {
            fx.clear_overlay = true;
        }
        match c {
            Cmd::Move if selected => self.toggle_tool(Tool::Move),
            Cmd::Route if selected => {
                self.draft.clear();
                self.toggle_tool(Tool::Route);
            }
            Cmd::Engage if self.sel_unit.is_some() => self.toggle_tool(Tool::Engage),
            Cmd::BattlePosition if self.sel_platoon.is_some() => {
                self.toggle_tool(Tool::BattlePosition { centre: None });
            }
            // One toggle: HALT keeps the route (or BP order); pressed again it is RESUME.
            Cmd::Halt => {
                if let Some(to) = self.addressee() {
                    self.draft.clear();
                    let cs = self.addressee_cs(to);
                    let resume = self.shown_held(to);
                    if resume {
                        self.order(Command::Resume { to });
                        self.radio(format!("TO {cs}: CONTINUE MISSION."));
                    } else {
                        self.order(Command::Halt { to });
                        self.radio(format!("TO {cs}: HALT."));
                    }
                    let busy = self.has_plan(to);
                    if self.order_mode == OrderMode::Now && (resume || busy) {
                        self.pending_held.retain(|x| x.0 != to);
                        self.pending_held.push((to, !resume, self.sim.time_s()));
                    }
                }
            }
            Cmd::Formation => {
                if let Some(p) = self.sel_platoon
                    && let Some(cur) = self.shown_formation(p)
                {
                    let i = Formation::ALL.iter().position(|f| *f == cur).unwrap_or(0);
                    let f = Formation::ALL[(i + 1) % Formation::ALL.len()];
                    if self.order_mode == OrderMode::Now {
                        self.pending_formation.retain(|x| x.0 != p);
                        self.pending_formation.push((p, f, self.sim.time_s()));
                    }
                    self.order(Command::SetFormation { platoon: p, formation: f });
                    let cs = self.platoon_cs(p);
                    self.radio(format!("TO {cs}: CHANGE FORMATION, {}.", format_formation(f)));
                }
            }
            Cmd::Technique => {
                self.technique = match self.technique {
                    Technique::Traveling => Technique::TravelingOverwatch,
                    Technique::TravelingOverwatch => Technique::BoundingOverwatch,
                    Technique::BoundingOverwatch => Technique::Traveling,
                };
            }
            // March speed for the next move; the selection is ordered to it as well (applies to
            // its current or held move).
            Cmd::Speed => {
                let presets = self.speed_presets();
                let i = presets.iter().position(|s| *s == self.move_speed).map_or(0, |i| (i + 1) % presets.len());
                self.move_speed = presets[i];
                if let Some(to) = self.addressee() {
                    let cs = self.addressee_cs(to);
                    self.order(Command::SetSpeed { to, speed_limit_mps: self.move_speed });
                    let sp = self.move_speed.map_or("MAX".into(), |v| format!("{} KM/H", format::kmh(v)));
                    self.radio(format!("TO {cs}: SPEED {sp}."));
                }
            }
            Cmd::Weapons => {
                if let Some(to) = self.addressee() {
                    let next = match self.selection_weapons().unwrap_or(WeaponsStatus::Free) {
                        WeaponsStatus::Free => WeaponsStatus::Tight,
                        WeaponsStatus::Tight => WeaponsStatus::Hold,
                        WeaponsStatus::Hold => WeaponsStatus::Free,
                    };
                    if self.order_mode == OrderMode::Now {
                        self.pending_weapons.retain(|x| x.0 != to);
                        self.pending_weapons.push((to, next, self.sim.time_s()));
                    }
                    self.order(Command::WeaponsControl { to, status: next });
                    self.radio(format!("WEAPONS {}.", weapons_str(next)));
                }
            }
            Cmd::Contingency => {
                self.order_mode = match self.order_mode {
                    OrderMode::Now => OrderMode::OnEnemyCrossing,
                    OrderMode::OnEnemyCrossing => OrderMode::InFiveMinutes,
                    OrderMode::InFiveMinutes => OrderMode::Now,
                };
            }
            Cmd::NextPlatoon => {
                let ps: Vec<(PlatoonId, Option<EntityId>)> =
                    self.picture().platoons.iter().map(|p| (p.id, p.members.first().copied())).collect();
                if !ps.is_empty() {
                    let i = self
                        .sel_platoon
                        .and_then(|s| ps.iter().position(|p| p.0 == s))
                        .map_or(0, |i| (i + 1) % ps.len());
                    self.sel_platoon = Some(ps[i].0);
                    self.sel_unit = ps[i].1;
                    self.draft.clear();
                }
            }
            Cmd::PhaseLine => self.toggle_tool_graphic(DrawKind::PhaseLine),
            Cmd::Objective => self.toggle_tool_graphic(DrawKind::Objective),
            Cmd::Trp => self.toggle_tool_graphic(DrawKind::Trp),
            Cmd::Viewshed => {
                self.tool = if self.tool == Tool::Viewshed { Tool::None } else { Tool::Viewshed };
                fx.clear_overlay = true;
            }
            Cmd::Los => self.toggle_tool(Tool::Los { a: None, b: None, result: None }),
            Cmd::Send => match self.tool.clone() {
                Tool::Graphic { kind: DrawKind::PhaseLine, points } if points.len() >= 2 => {
                    self.add_phase_line(points);
                    self.tool = Tool::Graphic { kind: DrawKind::PhaseLine, points: vec![] };
                }
                _ => self.send_route(),
            },
            Cmd::UndoPoint => {
                if let Tool::Graphic { points, .. } = &mut self.tool {
                    points.pop();
                } else {
                    self.draft.pop();
                }
            }
            Cmd::Cancel => {
                if self.tool != Tool::None || !self.draft.is_empty() {
                    self.tool = Tool::None;
                    self.draft.clear();
                } else {
                    self.sel_platoon = None;
                    self.sel_unit = None;
                }
            }
            Cmd::RadiusDown => self.bp_radius = (self.bp_radius / 1.25).max(100.0),
            Cmd::RadiusUp => self.bp_radius = (self.bp_radius * 1.25).min(800.0),
            Cmd::Move | Cmd::Route | Cmd::Engage | Cmd::BattlePosition => {
                self.radio("NO UNIT SELECTED.".into());
            }
        }
    }

    /// Selecting the active graphic tool again turns it off.
    fn toggle_tool_graphic(&mut self, kind: DrawKind) {
        match &self.tool {
            Tool::Graphic { kind: k, .. } if *k == kind => self.tool = Tool::None,
            _ => self.tool = Tool::Graphic { kind, points: vec![] },
        }
    }

    /// Weapons control status of the selection (its first vehicle).
    /// Weapons status of the selection, or the one just ordered if not yet acknowledged.
    fn selection_weapons(&self) -> Option<WeaponsStatus> {
        let to = self.addressee()?;
        if let Some(w) = self.pending_weapons.iter().find(|x| x.0 == to) {
            return Some(w.1);
        }
        self.actual_weapons(to)
    }

    /// Halted with an order held for RESUME, or as just ordered if not yet received.
    fn shown_held(&self, to: Addressee) -> bool {
        if let Some(h) = self.pending_held.iter().find(|x| x.0 == to) {
            return h.1;
        }
        self.actual_held(to)
    }

    fn actual_held(&self, to: Addressee) -> bool {
        let pic = self.picture();
        match to {
            Addressee::Unit(u) => pic.unit(u).is_some_and(|u| u.held),
            Addressee::Platoon(p) => pic.platoon(p).is_some_and(|v| v.held),
        }
    }

    /// Moving or occupying: something a HALT would hold.
    fn has_plan(&self, to: Addressee) -> bool {
        let pic = self.picture();
        match to {
            Addressee::Unit(u) => pic.unit(u).is_some_and(|u| u.platoon.is_none() && u.route.len() >= 2),
            Addressee::Platoon(p) => {
                pic.platoon(p).is_some_and(|v| !v.waypoints.is_empty() || v.task.starts_with("OCCUPYING"))
            }
        }
    }

    /// MAX, then the side's doctrine march speeds.
    fn speed_presets(&self) -> Vec<Option<f64>> {
        let mut v = vec![None];
        if let Some(d) = self.sim.doctrine(self.side) {
            v.extend(d.speed_limits_mps.iter().map(|s| Some(s.get())));
        }
        v
    }

    fn addressee_cs(&self, to: Addressee) -> String {
        match to {
            Addressee::Platoon(p) => self.platoon_cs(p),
            Addressee::Unit(u) => self.picture().unit(u).map_or("?".into(), |x| x.callsign.clone()),
        }
    }

    /// Platoon formation, or the one just ordered if not yet acknowledged.
    fn shown_formation(&self, p: PlatoonId) -> Option<Formation> {
        if let Some(f) = self.pending_formation.iter().find(|x| x.0 == p) {
            return Some(f.1);
        }
        self.picture().platoon(p).map(|v| v.formation)
    }

    /// Drop pending changes once the picture shows them (or after a minute, if overtaken).
    fn prune_pending(&mut self) {
        let now = self.sim.time_s();
        let fresh = |t: f64| now - t < 60.0;
        let mut pf = std::mem::take(&mut self.pending_formation);
        pf.retain(|&(p, f, t)| fresh(t) && self.picture().platoon(p).is_some_and(|v| v.formation != f));
        self.pending_formation = pf;
        let mut pw = std::mem::take(&mut self.pending_weapons);
        pw.retain(|&(to, w, t)| fresh(t) && self.actual_weapons(to).is_some_and(|a| a != w));
        self.pending_weapons = pw;
        let mut ph = std::mem::take(&mut self.pending_held);
        ph.retain(|&(to, h, t)| fresh(t) && self.actual_held(to) != h);
        self.pending_held = ph;
    }

    fn actual_weapons(&self, to: Addressee) -> Option<WeaponsStatus> {
        let pic = self.picture();
        match to {
            Addressee::Unit(u) => pic.unit(u).map(|u| u.weapons),
            Addressee::Platoon(p) => pic.platoon(p)?.members.first().and_then(|u| pic.unit(*u)).map(|u| u.weapons),
        }
    }

    /// Draw the next phase line (PL RED, PL BLUE, ...).
    pub fn add_phase_line(&mut self, points: Vec<[f64; 2]>) {
        self.counts[0] += 1;
        let label = format!("PL {}", PL_NAMES[(self.counts[0] as usize - 1) % PL_NAMES.len()]);
        self.add_graphic(label, SimGraphic::PhaseLine { points });
    }

    /// Leave the planning phase (H-hour).
    pub fn execute(&mut self) {
        if self.planning {
            self.planning = false;
            self.paused = false;
            self.radio("EXECUTE. H-HOUR.".into());
        }
    }

    pub fn open_aar(&mut self) {
        self.paused = true;
        self.aar = Some(AarView::new(&self.recorder));
    }

    fn add_graphic(&mut self, label: String, kind: SimGraphic) {
        self.sim.submit_immediate(
            self.side,
            Command::AddGraphic { graphic: Graphic { id: 0, label: label.clone(), kind } },
        );
        self.radio(format!("GRAPHIC {label} DISTRIBUTED."));
        self.sim.settle();
    }

    pub fn issue_bp(&mut self, centre: [f64; 2], facing: f64) {
        let Some(p) = self.sel_platoon else { return };
        self.draft.clear();
        self.order(Command::OccupyPosition {
            platoon: p,
            centre,
            radius_m: self.bp_radius,
            facing,
            kind: PositionKind::HullDown,
        });
        self.bp_count += 1;
        let label = format!("BP {}", self.bp_count);
        let cs = self.platoon_cs(p);
        self.radio(format!("TO {cs}: OCCUPY {label}, HULL-DOWN, ORIENT {} MILS.", format::mils(facing)));
        self.graphics.retain(|g| g.platoon != Some(p));
        self.graphics.push(OrderGraphic {
            platoon: Some(p),
            unit: None,
            kind: GraphicKind::Bp { centre, radius: self.bp_radius, facing, label },
            acknowledged: false,
        });
    }

    fn select(&mut self, cam: &Camera, s: P2, single: bool) {
        let hit = self
            .picture()
            .own
            .iter()
            .map(|u| {
                let p = cam.to_screen(u.pos);
                (u.id, u.platoon, (p[0] - s[0]).powi(2) + (p[1] - s[1]).powi(2))
            })
            .filter(|(_, _, d)| *d < 18.0 * 18.0)
            .min_by(|a, b| a.2.total_cmp(&b.2));
        match hit {
            Some((id, p, _)) => {
                self.sel_unit = Some(id);
                self.sel_platoon = if single { None } else { p };
            }
            None => {
                self.sel_unit = None;
                self.sel_platoon = None;
            }
        }
    }

    fn engage_at(&mut self, cam: &Camera, s: P2) {
        let Some(u) = self.sel_unit else { return };
        let c = self
            .picture()
            .live_contacts()
            .map(|c| {
                let p = cam.to_screen(c.pos);
                (c.id, (p[0] - s[0]).powi(2) + (p[1] - s[1]).powi(2))
            })
            .filter(|(_, d)| *d < 20.0 * 20.0)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((cid, _)) = c {
            self.order(Command::Engage { unit: u, contact: cid });
            self.radio("ENGAGE THAT TARGET.".into());
        }
    }

    fn eye_height(&self) -> f64 {
        self.sel_unit
            .and_then(|u| self.picture().unit(u))
            .and_then(|u| self.sim.data().vehicle(&u.vehicle_id))
            .map_or(2.3, |v| v.dimensions.sight_height_m.get())
    }

    fn viewshed(&self, at: [f64; 2]) -> (Vec<u8>, usize, usize, f64, f64, f64) {
        let map = self.sim.map();
        let g = Ground::new(map, &self.sim.data().terrain);
        let step = 8.0;
        let (vs, ox, oy) = terrain::los::viewshed(&g, at, self.eye_height(), 2.4, 3000.0, step);
        let (w, h) = (vs.w, vs.h);
        let mut rgba = vec![0u8; w * h * 4];
        for j in 0..h {
            for i in 0..w {
                let v = vs.get(i, j);
                let k = ((h - 1 - j) * w + i) * 4;
                let c: [u8; 4] = if v == 0 {
                    [60, 10, 10, 120]
                } else if v < 200 {
                    [140, 120, 40, 50]
                } else {
                    [0, 0, 0, 0]
                };
                rgba[k..k + 4].copy_from_slice(&c);
            }
        }
        (rgba, w, h, ox, oy, step)
    }

    fn los(&self, a: [f64; 2], b: [f64; 2]) -> (bool, f64) {
        let g = Ground::new(self.sim.map(), &self.sim.data().terrain);
        let s = terrain::los::sight(&g, a, self.eye_height(), b, &[2.4]);
        (s.any() && s.transmittance > 0.1, s.transmittance)
    }

    fn interp(&self, id: EntityId, pos: [f64; 2], heading: f64) -> ([f64; 2], f64) {
        let a = if self.paused { 1.0 } else { (self.acc / sim::DT).clamp(0.0, 1.0) };
        match self.prev.iter().find(|p| p.0 == id) {
            Some(&(_, pp, ph)) => {
                let dh = ai::drive::wrap(heading - ph);
                ([pp[0] + (pos[0] - pp[0]) * a, pp[1] + (pos[1] - pp[1]) * a], ph + dh * a)
            }
            None => (pos, heading),
        }
    }

    pub fn draw_world(&self, f: &mut Frame, cam: &Camera, mouse: P2) {
        if let Some(a) = &self.aar {
            a.draw_world(f, cam, &self.recorder);
            return;
        }
        let pic = self.picture();
        let pl = self.last_phase_line().map(|g| g.id);
        for g in &pic.graphics {
            symbols::graphic(f, cam, g, Some(g.id) == pl && self.order_mode == OrderMode::OnEnemyCrossing);
        }
        for g in &self.graphics {
            match &g.kind {
                GraphicKind::Bp { centre, radius, facing, label } => {
                    symbols::battle_position(f, cam, *centre, *radius, *facing, label, g.acknowledged);
                }
                // Sent, not yet received: dashed (DESIGN.md §10.3).
                GraphicKind::Route { points } => {
                    route_line(f, cam, points, style::ROUTE, true);
                    let skip = usize::from(points.len() > 1);
                    checkpoints(f, cam, &points[skip..], style::ROUTE);
                }
            }
        }
        // Platoon routes with checkpoints: solid while executing (DESIGN.md §10.3).
        for pv in &pic.platoons {
            let sel = self.sel_platoon == Some(pv.id);
            if pv.route.len() >= 2 {
                let c = if sel { style::ROUTE } else { Rgba(style::ROUTE.0, style::ROUTE.1, style::ROUTE.2, 90) };
                route_line(f, cam, &pv.route, c, false);
            }
            if sel || cam.mpp > AGGREGATE_MPP {
                checkpoints(f, cam, &pv.waypoints, style::ROUTE);
            }
        }
        // Single-vehicle routes (and slot routes of the selected platoon when zoomed in).
        for u in &pic.own {
            let detached = u.platoon.is_none() || self.sel_unit == Some(u.id) && self.sel_platoon.is_none();
            let slot = u.platoon.is_some() && u.platoon == self.sel_platoon && cam.mpp < 1.5;
            if u.route.len() >= 2 && (detached || slot) {
                route_line(f, cam, &u.route, style::ROUTE, false);
                f.circle_lines(cam.to_screen(*u.route.last().unwrap()), 4.0, style::ROUTE);
            }
        }
        // Route being drafted (Shift+right click or ROUTE tool), from the selection to the cursor.
        if !self.draft.is_empty() || self.tool == Tool::Route {
            let mut pts: Vec<[f64; 2]> = self.selection_centre().into_iter().collect();
            pts.extend(self.draft.iter().copied());
            if self.tool == Tool::Route {
                pts.push(cam.to_world(mouse));
            }
            route_line(f, cam, &pts, style::ROUTE_DRAFT, true);
            checkpoints(f, cam, &self.draft, style::ROUTE_DRAFT);
        }
        // Shots this side knows about.
        for s in &pic.shots {
            let age = (pic.time_s - s.time_s).clamp(0.0, 4.0);
            let a = (255.0 * (1.0 - age / 4.0)) as u8;
            let c = if s.own { Rgba(170, 220, 240, a) } else { Rgba(240, 150, 120, a) };
            f.line(cam.to_screen(s.from), cam.to_screen(s.to), 1.0, c);
            if s.hit && age < 1.0 {
                f.circle(cam.to_screen(s.to), 3.0, Rgba(250, 230, 160, a));
            }
        }
        // Contacts.
        for c in &pic.contacts {
            let p = cam.to_screen(c.pos);
            let age = pic.time_s - c.last_report_s;
            let fade = if age > 60.0 { 110 } else { 255 };
            let label = match (&c.type_name, c.level) {
                (Some(t), _) => t.clone(),
                (None, ContactLevel::Recognised) => "TANK".into(),
                (None, _) => "UNK".into(),
            };
            match c.level {
                ContactLevel::Detected => symbols::unknown_frame(f, p, Rgba(214, 196, 110, fade)),
                _ => symbols::hostile_armour(f, p, Rgba(style::HOSTILE.0, style::HOSTILE.1, style::HOSTILE.2, fade)),
            }
            if c.destroyed {
                symbols::destroyed(f, p, Rgba(220, 200, 180, fade));
            }
            let tag = if age > 60.0 { format!("{label} LKP {:.0}S", age) } else { label };
            f.text(
                &tag,
                p[0] + 18.0,
                p[1] - 5.0,
                style::FONT,
                Rgba(style::HOSTILE.0, style::HOSTILE.1, style::HOSTILE.2, fade),
            );
        }
        // Own units: individual vehicles, or platoon symbols when zoomed out.
        let aggregate = cam.mpp > AGGREGATE_MPP;
        let mut drawn_platoons = Vec::new();
        for u in &pic.own {
            if aggregate && let Some(pid) = u.platoon {
                if drawn_platoons.contains(&pid) {
                    continue;
                }
                drawn_platoons.push(pid);
                let Some(pv) = pic.platoon(pid) else { continue };
                let members: Vec<_> = pv
                    .members
                    .iter()
                    .filter_map(|m| pic.unit(*m))
                    .filter(|m| m.condition != Condition::Destroyed)
                    .collect();
                if members.is_empty() {
                    continue;
                }
                let cx = members.iter().map(|m| m.pos[0]).sum::<f64>() / members.len() as f64;
                let cy = members.iter().map(|m| m.pos[1]).sum::<f64>() / members.len() as f64;
                let p = cam.to_screen([cx, cy]);
                symbols::friendly_armour(f, p, style::FRIENDLY);
                symbols::echelon_platoon(f, p, style::FRIENDLY);
                f.text(
                    &format!("{} ({})", pv.callsign, members.len()),
                    p[0] + 16.0,
                    p[1] - 5.0,
                    style::FONT,
                    style::FRIENDLY,
                );
                if self.sel_platoon == Some(pid) {
                    symbols::selection(f, p);
                }
                continue;
            }
            let (pos, hdg) = self.interp(u.id, u.pos, u.heading);
            let p = cam.to_screen(pos);
            if cam.mpp < 0.6 {
                symbols::hull(f, cam, pos, hdg, u.turret, 7.9, 3.7, style::FRIENDLY);
            }
            symbols::friendly_armour(f, p, style::FRIENDLY);
            match u.condition {
                Condition::Destroyed => symbols::destroyed(f, p, style::FRIENDLY),
                Condition::MobilityKill | Condition::FirepowerKill => symbols::damaged(f, p, style::AMBER),
                Condition::Operational => {}
            }
            if u.speed > 0.3 {
                symbols::heading_staff(f, p, hdg, 14.0, style::FRIENDLY);
            }
            f.text(&u.callsign, p[0] + 16.0, p[1] - 5.0, style::FONT, style::FRIENDLY);
            let selected = self.sel_unit == Some(u.id) || (self.sel_platoon.is_some() && u.platoon == self.sel_platoon);
            if selected {
                symbols::selection(f, p);
            }
        }
        #[cfg(feature = "debug-truth")]
        self.draw_truth(f, cam);
        self.draw_tool(f, cam, mouse);
    }

    fn draw_tool(&self, f: &mut Frame, cam: &Camera, mouse: P2) {
        match &self.tool {
            Tool::Move => {
                if let Some(c) = self.selection_centre() {
                    route_line(f, cam, &[c, cam.to_world(mouse)], style::ROUTE_DRAFT, true);
                }
                f.circle_lines(mouse, 6.0, style::ROUTE_DRAFT);
            }
            Tool::Engage => {
                f.circle_lines(mouse, 10.0, style::HOSTILE);
                f.line([mouse[0] - 14.0, mouse[1]], [mouse[0] + 14.0, mouse[1]], 1.0, style::HOSTILE);
                f.line([mouse[0], mouse[1] - 14.0], [mouse[0], mouse[1] + 14.0], 1.0, style::HOSTILE);
            }
            Tool::BattlePosition { centre } => {
                let c = centre.unwrap_or_else(|| cam.to_world(mouse));
                let facing = centre.map_or(0.0, |c| ai::drive::bearing(c, cam.to_world(mouse)));
                symbols::battle_position(f, cam, c, self.bp_radius, facing, "BP", false);
            }
            Tool::Graphic { kind, points } => {
                let m = cam.to_world(mouse);
                match kind {
                    DrawKind::PhaseLine if !points.is_empty() => {
                        let mut pts: Vec<P2> = points.iter().map(|&p| cam.to_screen(p)).collect();
                        pts.push(mouse);
                        for w in pts.windows(2) {
                            f.line(w[0], w[1], 1.5, style::EDIT_CURSOR);
                        }
                    }
                    DrawKind::Objective if !points.is_empty() => {
                        let r =
                            (((m[0] - points[0][0]).powi(2) + (m[1] - points[0][1]).powi(2)).sqrt() / cam.mpp) as f32;
                        f.circle_lines(cam.to_screen(points[0]), r, style::EDIT_CURSOR);
                    }
                    _ => f.circle_lines(mouse, 4.0, style::EDIT_CURSOR),
                }
            }
            Tool::Los { a: Some(a), b, result } => {
                let b = b.unwrap_or_else(|| cam.to_world(mouse));
                let c = match result {
                    Some((true, _)) => Rgba(120, 220, 140, 255),
                    Some((false, _)) => Rgba(230, 110, 100, 255),
                    None => style::EDIT_CURSOR,
                };
                f.line(cam.to_screen(*a), cam.to_screen(b), 1.5, c);
                let d = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
                let txt = match result {
                    Some((ok, t)) => format!(
                        "{} {} {} MILS  T={:.2}",
                        if *ok { "LOS" } else { "NO LOS" },
                        format::distance(d),
                        format::mils(ai::drive::bearing(*a, b)),
                        t
                    ),
                    None => format!("{} {} MILS", format::distance(d), format::mils(ai::drive::bearing(*a, b))),
                };
                let s = cam.to_screen(b);
                f.text(&txt, s[0] + 8.0, s[1] + 6.0, style::FONT, c);
            }
            _ => {}
        }
    }

    #[cfg(feature = "debug-truth")]
    fn draw_truth(&self, f: &mut Frame, cam: &Camera) {
        let w = self.sim.truth();
        for k in 0..w.len() {
            if w.side[k] != self.side {
                let p = cam.to_screen(w.transform[k].pos);
                f.circle_lines(p, 9.0, Rgba(255, 60, 200, 255));
                f.text(
                    &format!("{} [TRUTH]", w.callsign[k]),
                    p[0] + 12.0,
                    p[1] + 6.0,
                    style::FONT,
                    Rgba(255, 60, 200, 255),
                );
            }
        }
    }

    /// Prompt for the active tool or draft (shown under the top bar).
    fn prompt(&self) -> String {
        let n = self.draft.len();
        match &self.tool {
            Tool::None if n > 0 => format!("ROUTE: {n} POINT(S). SHIFT+RIGHT CLICK ADDS, RELEASE SHIFT TO SEND"),
            Tool::None => String::new(),
            Tool::Move => "MOVE: CLICK DESTINATION   (RIGHT CLICK / ESC CANCEL)".into(),
            Tool::Route => format!("ROUTE: CLICK CHECKPOINTS ({n}), RIGHT CLICK DESTINATION   BKSP UNDO   ESC CANCEL"),
            Tool::Engage => "ENGAGE: CLICK A CONTACT   (RIGHT CLICK / ESC CANCEL)".into(),
            Tool::BattlePosition { centre: None } => "BATTLE POSITION: CLICK CENTRE   [ ] RADIUS".into(),
            Tool::BattlePosition { .. } => "BATTLE POSITION: CLICK THREAT DIRECTION".into(),
            Tool::Viewshed => "VIEWSHED: CLICK OBSERVER POINT   (V / ESC CLOSE)".into(),
            Tool::Los { .. } => "LINE OF SIGHT: CLICK TWO POINTS".into(),
            Tool::Graphic { kind: DrawKind::PhaseLine, .. } => {
                "PHASE LINE: CLICK POINTS, ENTER TO FINISH   BKSP UNDO   ESC CANCEL".into()
            }
            Tool::Graphic { kind: DrawKind::Objective, .. } => "OBJECTIVE: CLICK CENTRE, THEN EDGE".into(),
            Tool::Graphic { kind: DrawKind::Trp, .. } => "TARGET REFERENCE POINT: CLICK POINTS".into(),
        }
    }

    fn state_text(&self) -> (String, Rgba) {
        if self.planning {
            ("PLANNING".into(), style::AMBER)
        } else if self.paused {
            ("PAUSED".into(), style::AMBER)
        } else {
            (format!("RUNNING X{}", SPEEDS[self.speed]), style::TEXT)
        }
    }

    /// Every clickable control of game mode, laid out for this screen size.
    pub fn buttons(&self, screen: P2) -> Vec<Button<GameAction>> {
        let mut out = Vec::new();
        let aar = self.aar.is_some();
        // Top bar: time control.
        let mut bar = Vec::new();
        if !aar {
            if self.planning {
                bar.push(
                    Button::new(Rect::new(0.0, 0.0, 0.0, 0.0), Look::Bar, "EXECUTE", GameAction::Execute)
                        .key("ENTER")
                        .tip("END PLANNING: START THE CLOCK (H-HOUR)")
                        .colour(style::AMBER),
                );
            } else {
                let label = if self.paused { "RESUME" } else { "PAUSE" };
                bar.push(
                    Button::new(Rect::new(0.0, 0.0, 0.0, 0.0), Look::Bar, label, GameAction::Pause)
                        .key("SPACE")
                        .tip("PAUSE / RESUME (ORDERS CAN BE GIVEN WHILE PAUSED)")
                        .active(self.paused),
                );
            }
            for (k, x) in SPEEDS.iter().enumerate() {
                bar.push(
                    Button::new(Rect::new(0.0, 0.0, 0.0, 0.0), Look::Bar, format!("X{x}"), GameAction::Speed(k))
                        .key(["1", "2", "3", "4"][k])
                        .tip(format!("TIME COMPRESSION X{x}"))
                        .active(self.speed == k),
                );
            }
        }
        bar.push(
            Button::new(Rect::new(0.0, 0.0, 0.0, 0.0), Look::Bar, "AAR", GameAction::Aar)
                .key("F8")
                .tip("AFTER-ACTION REVIEW: REPLAY THE BATTLE WITH GROUND TRUTH")
                .active(aar),
        );
        hud::bar_row(&mut out, 190.0, bar);
        if aar {
            return out;
        }

        // Platoon roster (left).
        for (k, (label, value, act, sel)) in self.roster().into_iter().enumerate() {
            let r = Rect::new(8.0, TOP_H + 26.0 + k as f32 * 24.0, ROSTER_W - 4.0, 22.0);
            out.push(
                Button::new(r, Look::Row, label, act)
                    .value(value)
                    .active(sel)
                    .colour(style::FRIENDLY)
                    .tip("SELECT (CLICK AGAIN TO CENTRE THE MAP)")
                    .key("TAB"),
            );
        }

        // Selection panel: member rows select a single vehicle.
        let (_, _, rows, _) = self.info(screen);
        out.extend(rows);

        // Command card.
        for (i, b) in self.card() {
            let mut b = b;
            b.r = hud::card_cell(screen, i);
            out.push(b);
        }
        out
    }

    /// Roster rows: (label, value, action, selected).
    fn roster(&self) -> Vec<(String, String, GameAction, bool)> {
        let pic = self.picture();
        let mut rows = Vec::new();
        for pv in &pic.platoons {
            let alive = pv.members.iter().filter_map(|m| pic.unit(*m)).filter(|u| u.condition != Condition::Destroyed);
            let n = alive.count();
            let task = pv.task.replace("BATTLE POSITION", "BP");
            rows.push((
                format!("{} PLT  {}/{}", pv.callsign, n, pv.members.len()),
                task,
                GameAction::SelectPlatoon(pv.id),
                self.sel_platoon == Some(pv.id),
            ));
        }
        for u in pic.own.iter().filter(|u| u.platoon.is_none()) {
            rows.push((
                u.callsign.clone(),
                short_task(u),
                GameAction::SelectUnit(u.id),
                self.sel_unit == Some(u.id) && self.sel_platoon.is_none(),
            ));
        }
        rows
    }

    /// Command card cells (index, button), StarCraft-style: a fixed 5 × 3 grid, with a
    /// sub-card while a route, phase line or battle position is being placed.
    fn card(&self) -> Vec<(usize, Button<GameAction>)> {
        let z = Rect::new(0.0, 0.0, 0.0, 0.0);
        let b = |label: &str, c: Cmd| Button::new(z, Look::Card, label, GameAction::Cmd(c));
        let sel = self.addressee().is_some();
        let plt = self.sel_platoon.is_some();
        let cancel = (14, b("CANCEL", Cmd::Cancel).key("ESC").tip("CANCEL (RIGHT CLICK ALSO CANCELS)"));
        let timing = match self.order_mode {
            OrderMode::Now => "NOW".to_owned(),
            OrderMode::OnEnemyCrossing => {
                format!("ON {}", self.last_phase_line().map_or("PL ?".into(), |g| g.label.clone()))
            }
            OrderMode::InFiveMinutes => "AT +5 MIN".into(),
        };
        let technique = || {
            b("TECHNIQUE", Cmd::Technique)
                .key("T")
                .value(short_technique(self.technique))
                .tip("MOVEMENT TECHNIQUE FOR THE NEXT MOVE: TRAVELING / TRAVELING OVERWATCH / BOUNDING OVERWATCH")
        };
        let contingency = || {
            b("TIMING", Cmd::Contingency)
                .key("K")
                .value(timing.clone())
                .active(self.order_mode != OrderMode::Now)
                .tip("NEXT ORDER: NOW / WHEN ENEMY CROSSES THE LAST PHASE LINE / IN 5 MIN")
        };
        if self.tool == Tool::Route || !self.draft.is_empty() {
            let n = self.draft.len();
            return vec![
                (
                    0,
                    b("SEND", Cmd::Send)
                        .key("ENTER")
                        .value(format!("{n} PTS"))
                        .enabled(n > 0)
                        .tip("SEND THE ROUTE: LAST POINT IS THE DESTINATION"),
                ),
                (1, b("UNDO PT", Cmd::UndoPoint).key("BKSP").enabled(n > 0).tip("REMOVE THE LAST POINT")),
                (3, technique()),
                (4, contingency()),
                cancel,
            ];
        }
        if let Tool::Graphic { kind: DrawKind::PhaseLine, points } = &self.tool
            && !points.is_empty()
        {
            return vec![
                (0, b("FINISH", Cmd::Send).key("ENTER").enabled(points.len() >= 2).tip("FINISH THE PHASE LINE")),
                (1, b("UNDO PT", Cmd::UndoPoint).key("BKSP").tip("REMOVE THE LAST POINT")),
                cancel,
            ];
        }
        if let Tool::BattlePosition { .. } = self.tool {
            let r = format!("{:.0} M", self.bp_radius);
            return vec![
                (0, b("RADIUS -", Cmd::RadiusDown).key("[").value(r.clone()).tip("SMALLER BATTLE POSITION")),
                (1, b("RADIUS +", Cmd::RadiusUp).key("]").value(r).tip("LARGER BATTLE POSITION")),
                (4, contingency()),
                cancel,
            ];
        }
        let is = |t: &Tool| std::mem::discriminant(&self.tool) == std::mem::discriminant(t);
        let gfx = |k: DrawKind| matches!(&self.tool, Tool::Graphic { kind, .. } if *kind == k);
        let formation = self.sel_platoon.and_then(|p| self.shown_formation(p)).map_or(String::new(), format_formation);
        let weapons = self.selection_weapons().map_or("", weapons_str);
        vec![
            (
                0,
                b("MOVE", Cmd::Move)
                    .key("M")
                    .value("R-CLICK")
                    .enabled(sel)
                    .active(is(&Tool::Move))
                    .tip("MOVE TO A POINT (OR RIGHT CLICK THE MAP)"),
            ),
            (
                1,
                b("ROUTE", Cmd::Route)
                    .key("R")
                    .value("SHIFT+R-CLK")
                    .enabled(sel)
                    .active(is(&Tool::Route))
                    .tip("ROUTE WITH CHECKPOINTS: CLICK CPS, RIGHT CLICK DESTINATION (OR SHIFT+RIGHT CLICKS)"),
            ),
            (2, {
                let resume = self.addressee().is_some_and(|to| self.shown_held(to));
                if resume {
                    b("RESUME", Cmd::Halt).key("H").value("ROUTE HELD").tip("CONTINUE THE HALTED ROUTE OR BP ORDER")
                } else {
                    b("HALT", Cmd::Halt)
                        .key("H")
                        .enabled(sel)
                        .tip("HALT IN PLACE (KEEPS THE ROUTE: PRESS AGAIN TO RESUME)")
                }
            }),
            (
                3,
                b("BATTLE POS", Cmd::BattlePosition)
                    .key("B")
                    .value(format!("{:.0} M", self.bp_radius))
                    .enabled(plt)
                    .active(is(&Tool::BattlePosition { centre: None }))
                    .tip("OCCUPY A BATTLE POSITION HULL-DOWN: CLICK CENTRE, THEN THREAT DIRECTION"),
            ),
            (
                4,
                b("ENGAGE", Cmd::Engage)
                    .key("A")
                    .value("CTRL+R-CLK")
                    .enabled(self.sel_unit.is_some())
                    .active(is(&Tool::Engage))
                    .tip("SELECTED TANK ENGAGES A CONTACT (PLATOONS DISTRIBUTE FIRE THEMSELVES)"),
            ),
            (
                5,
                b("FORMATION", Cmd::Formation)
                    .key("F")
                    .value(formation)
                    .enabled(plt)
                    .tip("CHANGE PLATOON FORMATION (CYCLES)"),
            ),
            (6, technique()),
            (
                7,
                b("WEAPONS", Cmd::Weapons)
                    .key("W")
                    .value(weapons)
                    .enabled(sel)
                    .tip("WEAPONS CONTROL: FREE / TIGHT / HOLD"),
            ),
            (8, contingency()),
            (
                9,
                b("MOVE SPEED", Cmd::Speed)
                    .key("S")
                    .value(speed_short(self.move_speed))
                    .tip("MARCH SPEED: ORDERS THE SELECTION AND SETS THE NEXT MOVE (MAX / DOCTRINE SPEEDS)"),
            ),
            (
                10,
                b("PHASE LINE", Cmd::PhaseLine)
                    .key("G")
                    .active(gfx(DrawKind::PhaseLine))
                    .tip("DRAW A PHASE LINE (CONTROL MEASURE AND TRIGGER REFERENCE)"),
            ),
            (11, b("OBJECTIVE", Cmd::Objective).key("G").active(gfx(DrawKind::Objective)).tip("DRAW AN OBJECTIVE")),
            (12, b("TRP", Cmd::Trp).key("G").active(gfx(DrawKind::Trp)).tip("PLACE TARGET REFERENCE POINTS")),
            (
                13,
                b("VIEWSHED", Cmd::Viewshed)
                    .key("V")
                    .active(self.tool == Tool::Viewshed)
                    .tip("SHOW WHAT CAN BE SEEN FROM A POINT"),
            ),
            (
                14,
                b("LOS", Cmd::Los)
                    .key("L")
                    .active(is(&Tool::Los { a: None, b: None, result: None }))
                    .tip("LINE OF SIGHT BETWEEN TWO POINTS"),
            ),
        ]
    }

    /// Selection panel (top right): frame, header lines, member rows (and the DATA button),
    /// footer lines. A single vehicle gets a full status card; a platoon a summary and one row
    /// per vehicle.
    #[allow(clippy::type_complexity)]
    fn info(&self, screen: P2) -> (Rect, Vec<(String, Rgba)>, Vec<Button<GameAction>>, Vec<(String, Rgba)>) {
        const W: f32 = 320.0;
        const LINE: f32 = 18.0;
        let (x, y) = (screen[0] - W - 6.0, TOP_H + 6.0);
        let pic = self.picture();
        let mut head: Vec<(String, Rgba)> = Vec::new();
        let mut rows = Vec::new();
        let mut members = Vec::new();
        let mut back_to: Option<(PlatoonId, String)> = None;
        if let Some(pv) = self.sel_platoon.and_then(|p| pic.platoon(p)) {
            members = pv.members.iter().filter_map(|m| pic.unit(*m)).collect();
            let mut types: Vec<(String, usize)> = Vec::new();
            for u in &members {
                match types.iter_mut().find(|(n, _)| *n == u.vehicle_name) {
                    Some(t) => t.1 += 1,
                    None => types.push((u.vehicle_name.clone(), 1)),
                }
            }
            let types: Vec<String> = types.iter().map(|(n, k)| format!("{k}× {n}")).collect();
            head.push((format!("PLATOON {}   {}", pv.callsign, types.join(", ")), style::FRIENDLY));
            head.push((pv.task.clone(), style::TEXT));
            head.push((
                format!(
                    "FORM {}   SPEED {}   NEXT {}, {}",
                    format_formation(self.shown_formation(pv.id).unwrap_or(pv.formation)),
                    speed_short(pv.speed_limit),
                    short_technique(self.technique),
                    speed_short(self.move_speed)
                ),
                style::TEXT_DIM,
            ));
            if !pv.waypoints.is_empty() {
                head.push((
                    format!(
                        "ROUTE {} CP, DEST GRID {}",
                        pv.waypoints.len() - 1,
                        format::grid(*pv.waypoints.last().unwrap())
                    ),
                    style::TEXT_DIM,
                ));
            }
            let up = members.iter().filter(|u| u.condition == Condition::Operational).count();
            let rounds: u32 = members.iter().filter(|u| u.condition != Condition::Destroyed).map(|u| u.ammo).sum();
            let fuel = members
                .iter()
                .filter(|u| u.condition != Condition::Destroyed)
                .map(|u| u.fuel_fraction)
                .fold(f64::INFINITY, f64::min);
            let fuel = if fuel.is_finite() { format!("{:.0} %", fuel * 100.0) } else { "-".into() };
            head.push((
                format!("UP {up}/{}   MAIN GUN {rounds} RDS   FUEL LOW {fuel}", members.len()),
                if up < members.len() { style::AMBER } else { style::TEXT },
            ));
        } else if let Some(u) = self.sel_unit.and_then(|id| pic.unit(id)) {
            head = self.unit_card(u, pic);
            if let Some(p) = u.platoon {
                back_to = Some((p, self.platoon_cs(p)));
            }
        } else {
            head.push(("NO UNIT SELECTED".into(), style::TEXT_DIM));
            head.push(("CLICK A UNIT OR A PLATOON IN THE ROSTER".into(), style::TEXT_DIM));
        }
        if self.sel_unit.is_some() {
            rows.push(
                Button::new(Rect::new(x + W - 64.0, y + 5.0, 58.0, 20.0), Look::Bar, "DATA", GameAction::VehicleData)
                    .key("I")
                    .tip("VEHICLE DATA AND SOURCES (DATABASE)"),
            );
        }
        let mut cy = y + 8.0 + head.len() as f32 * LINE + 4.0;
        for m in members {
            let label = format!("{:<5} {:>3} KM/H {:>3} RDS", m.callsign, format::kmh(m.speed), m.ammo);
            rows.push(
                Button::new(Rect::new(x + 6.0, cy, W - 12.0, 20.0), Look::Row, label, GameAction::SelectUnit(m.id))
                    .value(short_task(m))
                    .colour(condition_colour(m.condition))
                    .active(self.sel_unit == Some(m.id) && self.sel_platoon.is_none())
                    .tip("SELECT THIS VEHICLE ALONE: FULL STATUS (CLICK AGAIN TO CENTRE)"),
            );
            cy += 22.0;
        }
        if let Some((p, cs)) = back_to {
            rows.push(
                Button::new(
                    Rect::new(x + 6.0, cy, W - 12.0, 20.0),
                    Look::Row,
                    format!("PLATOON {cs}"),
                    GameAction::SelectPlatoon(p),
                )
                .value("SELECT")
                .colour(style::FRIENDLY)
                .tip("SELECT THE WHOLE PLATOON"),
            );
            cy += 22.0;
        }
        let mut foot: Vec<(String, Rgba)> = Vec::new();
        for t in pic.triggers.iter().rev().take(3) {
            let cond = match &t.condition {
                TriggerCondition::EnemyCrosses { line } => {
                    format!("ENEMY X {}", pic.graphic(*line).map_or("PL".into(), |g| g.label.clone()))
                }
                TriggerCondition::FriendlyCrosses { line, .. } => {
                    format!("OWN X {}", pic.graphic(*line).map_or("PL".into(), |g| g.label.clone()))
                }
                TriggerCondition::AtTime { time_s } => {
                    format!("AT {}", format::clock(self.scenario.start_time_s as f64 + time_s))
                }
            };
            foot.push((
                format!("CW {}: {cond} -> {}{}", t.id, t.order, if t.fired { " (DONE)" } else { "" }),
                style::TEXT_DIM,
            ));
        }
        foot.push((
            format!(
                "CONTACTS {}   KILLS {}",
                pic.live_contacts().count(),
                pic.contacts.iter().filter(|c| c.destroyed).count()
            ),
            style::TEXT_DIM,
        ));
        let h = cy - y + 4.0 + foot.len() as f32 * LINE + 6.0;
        (Rect::new(x, y, W, h), head, rows, foot)
    }

    /// Status card of one own vehicle: what its crew reports (own units are reported exactly).
    fn unit_card(&self, u: &OwnUnit, pic: &PerceivedWorld) -> Vec<(String, Rgba)> {
        let ds = self.sim.data();
        let def = ds.vehicle(&u.vehicle_id);
        let year = self.scenario.date.0;
        let mut out = vec![(format!("{}   {}", u.callsign, u.vehicle_name), style::FRIENDLY)];
        let state = match u.condition {
            Condition::Operational => "OPERATIONAL",
            Condition::MobilityKill => "MOBILITY KILL",
            Condition::FirepowerKill => "FIREPOWER KILL",
            Condition::Destroyed => "DESTROYED",
        };
        let crew_c = if u.crew.0 < u.crew.1 { style::AMBER } else { condition_colour(u.condition) };
        out.push((format!("{state}   CREW {}/{}", u.crew.0, u.crew.1), crew_c));
        let row = |k: &str, v: String| format!("{k:<8} {v}");
        let mut task = short_task(u);
        if let (Task::Set, Some(n)) = (&u.task, &u.position_note)
            && !task.contains(n.as_str())
        {
            task = format!("{task} · {n}");
        }
        out.push((row("TASK", task), style::TEXT));
        out.push((row("GRID", format!("{}   HDG {}", format::grid8(u.pos), format::mils(u.heading))), style::TEXT));
        // Ordered limit: the vehicle's own move, else its platoon's march speed.
        let limit = u.speed_limit.or_else(|| u.platoon.and_then(|p| pic.platoon(p)).and_then(|v| v.speed_limit));
        let top = match (limit, def) {
            (Some(l), _) => format!(" (LIMIT {})", format::kmh(l)),
            (None, Some(d)) => format!(" (ROAD {})", format::kmh(d.mobility.max_speed_road_mps.get())),
            _ => String::new(),
        };
        out.push((row("SPEED", format!("{} KM/H{top}", format::kmh(u.speed))), style::TEXT));
        let litres =
            def.map_or(String::new(), |d| format!("   {:.0} L", u.fuel_fraction * d.mobility.fuel_capacity_l.get()));
        let fuel_c = if u.fuel_fraction < 0.25 { style::AMBER } else { style::TEXT };
        out.push((row("FUEL", format!("{:.0} %{litres}", u.fuel_fraction * 100.0)), fuel_c));
        let load: u32 = def.map_or(0, |d| d.armament.load.iter().map(|l| l.count).sum());
        let gun = match (u.condition, u.gun) {
            (Condition::FirepowerKill | Condition::Destroyed, _) => "OUT OF ACTION",
            (_, GunActivity::Ready) => "READY",
            (_, GunActivity::Laying) => "LAYING",
            (_, GunActivity::Lasing) => "LASING",
            (_, GunActivity::Reloading) => "RELOADING",
        };
        let rounds = def
            .and_then(|d| d.armament.load.first())
            .and_then(|l| ds.ammo(&l.ammo))
            .map_or(String::new(), |a| format!(" {}", a.name));
        let ammo_c = if load > 0 && u.ammo * 4 < load { style::AMBER } else { style::TEXT };
        out.push((row("MAIN GUN", format!("{}/{load}{rounds}   {gun}", u.ammo)), ammo_c));
        out.push((
            row("TURRET", format!("{}   WEAPONS {}", format::mils(u.turret), weapons_str(u.weapons))),
            style::TEXT,
        ));
        let target = u.target.and_then(|c| pic.contact(c)).map_or("NONE".to_owned(), |c| {
            let what = match (&c.type_name, c.level) {
                (Some(t), _) => t.clone(),
                (None, ContactLevel::Recognised) => "TANK".into(),
                (None, _) => "UNKNOWN".into(),
            };
            let d = ((c.pos[0] - u.pos[0]).powi(2) + (c.pos[1] - u.pos[1]).powi(2)).sqrt();
            format!("{what}, {} DIR {}", format::distance(d), format::mils(ai::drive::bearing(u.pos, c.pos)))
        });
        out.push((row("TARGET", target), if u.target.is_some() { style::HOSTILE } else { style::TEXT_DIM }));
        let mut kinds: Vec<&str> = Vec::new();
        for s in def.iter().flat_map(|d| d.sensors.iter()).filter(|s| s.in_service.contains(year)) {
            let k = match s.kind {
                sim_data::SensorKind::Optical => "DAY",
                sim_data::SensorKind::NightChannel => "NIGHT",
                sim_data::SensorKind::Thermal => "THERMAL",
            };
            if !kinds.contains(&k) {
                kinds.push(k);
            }
        }
        let sights = kinds.join(" · ");
        if u.sights_degraded {
            out.push((row("SIGHTS", format!("DAMAGED ({sights})")), style::AMBER));
        } else {
            out.push((row("SIGHTS", sights), style::TEXT));
        }
        out
    }

    /// Panels that block map clicks (besides buttons and the top bar).
    pub fn panels(&self, screen: P2) -> Vec<Rect> {
        if self.aar.is_some() {
            return Vec::new();
        }
        let n = self.roster().len() as f32;
        vec![
            self.info(screen).0,
            Rect::new(4.0, TOP_H + 4.0, ROSTER_W + 4.0, 24.0 + n * 24.0),
            hud::minimap_frame(screen),
            hud::log_frame(screen),
            hud::card_frame(screen),
        ]
    }

    /// Top bar (game part), panels and the command card. Buttons are drawn by the caller.
    pub fn draw_ui(&self, f: &mut Frame, screen: P2) {
        let (state, sc) = self.state_text();
        f.text(&format::clock(self.sim.clock_s()), 10.0, 8.0, style::FONT_L - 4, style::TEXT);
        f.text(&state, 104.0, 10.0, style::FONT_S, sc);
        if let Some(a) = &self.aar {
            a.draw_ui(f, screen, &self.recorder, self.sim.side_names(), self.scenario.start_time_s as f64);
            return;
        }
        let side = &self.sim.side_names()[self.side.0 as usize];
        f.text(&format!("{side}  /  {}", self.scenario.name), 470.0, 10.0, style::FONT_S, style::TEXT_DIM);

        let prompt = self.prompt();
        let mode = match self.order_mode {
            OrderMode::Now => String::new(),
            OrderMode::OnEnemyCrossing => format!(
                "NEXT ORDER ON ENEMY CROSSING {}",
                self.last_phase_line().map_or("(NO PHASE LINE)".into(), |g| g.label.clone())
            ),
            OrderMode::InFiveMinutes => "NEXT ORDER AT +5 MIN".into(),
        };
        let mut py = TOP_H + 6.0;
        // (The end-of-mission panel is the app's: it carries menu buttons.)
        for (t, c) in [(prompt, style::AMBER), (mode, style::SELECT)] {
            if t.is_empty() {
                continue;
            }
            let w = f.text_width(&t, style::FONT_S) + 24.0;
            let x = (screen[0] - w) / 2.0;
            hud::panel(f, Rect::new(x, py, w, 24.0));
            f.text(&t, x + 12.0, py + 6.0, style::FONT_S, c);
            py += 28.0;
        }

        // Roster.
        let n = self.roster().len() as f32;
        hud::panel(f, Rect::new(4.0, TOP_H + 4.0, ROSTER_W + 4.0, 24.0 + n * 24.0));
        f.text("ORDER OF BATTLE", 12.0, TOP_H + 10.0, style::FONT, style::TEXT_DIM);

        // Selection panel.
        let (r, head, _, foot) = self.info(screen);
        hud::panel(f, r);
        for (k, (l, c)) in head.iter().enumerate() {
            f.text(l, r.x + 10.0, r.y + 8.0 + k as f32 * 18.0, style::FONT_S, *c);
        }
        for (k, (l, c)) in foot.iter().enumerate() {
            let y = r.y + r.h - 6.0 - (foot.len() - k) as f32 * 18.0;
            f.text(l, r.x + 10.0, y + 3.0, style::FONT_S, *c);
        }

        // Radio log.
        let lr = hud::log_frame(screen);
        hud::panel(f, lr);
        f.text("RADIO NET", lr.x + 8.0, lr.y + 5.0, style::FONT, style::TEXT_DIM);
        let n = ((lr.h - 24.0) / 16.0) as usize;
        let max_chars = ((lr.w - 16.0) / (style::FONT_S as f32 * 0.62)) as usize;
        let mut lines: Vec<String> = Vec::new();
        for (t, msg) in self.log.iter().rev() {
            // Wrap long transmissions; newest at the bottom.
            let full = format!("{t}  {msg}");
            let mut chunk: Vec<String> = Vec::new();
            let mut cur = String::new();
            for w in full.split(' ') {
                if !cur.is_empty() && cur.len() + 1 + w.len() > max_chars {
                    chunk.push(std::mem::take(&mut cur));
                    cur.push_str("          ");
                }
                if !cur.is_empty() && !cur.ends_with(' ') {
                    cur.push(' ');
                }
                cur.push_str(w);
            }
            chunk.push(cur);
            for c in chunk.into_iter().rev() {
                lines.push(c);
            }
            if lines.len() >= n {
                break;
            }
        }
        for (k, l) in lines.iter().take(n).rev().enumerate() {
            f.text(l, lr.x + 8.0, lr.y + 20.0 + k as f32 * 16.0, style::FONT_S, style::TEXT);
        }

        // Command card frame and title.
        let cr = hud::card_frame(screen);
        hud::panel(f, cr);
        let title = match (self.sel_platoon, self.sel_unit) {
            (Some(p), _) => format!("ORDERS: PLATOON {}", self.platoon_cs(p)),
            (None, Some(u)) => format!("ORDERS: {}", self.picture().unit(u).map_or("?".into(), |u| u.callsign.clone())),
            _ => "ORDERS: NO SELECTION".into(),
        };
        f.text(&title, cr.x + 8.0, cr.y + 6.0, style::FONT, style::TEXT_DIM);
    }

    /// Own units, contacts and routes on the minimap.
    pub fn draw_minimap(&self, f: &mut Frame, m: &Minimap) {
        if self.aar.is_some() {
            return;
        }
        let pic = self.picture();
        for g in &pic.graphics {
            if let SimGraphic::PhaseLine { points } = &g.kind {
                for w in points.windows(2) {
                    f.line(m.to_px(w[0]), m.to_px(w[1]), 1.0, style::TEXT_DIM);
                }
            }
        }
        for pv in &pic.platoons {
            for w in pv.route.windows(2) {
                f.line(m.to_px(w[0]), m.to_px(w[1]), 1.0, style::ROUTE);
            }
        }
        for c in pic.live_contacts() {
            let p = m.to_px(c.pos);
            f.rect(p[0] - 2.0, p[1] - 2.0, 4.0, 4.0, style::HOSTILE);
        }
        for u in &pic.own {
            let p = m.to_px(u.pos);
            let c = if u.condition == Condition::Destroyed { style::TEXT_OFF } else { style::FRIENDLY };
            let sel = self.sel_unit == Some(u.id) || (u.platoon.is_some() && u.platoon == self.sel_platoon);
            f.circle(p, if sel { 3.0 } else { 2.0 }, if sel { style::SELECT } else { c });
        }
    }
}

const ROSTER_W: f32 = 236.0;

/// Polyline in world coordinates; dashed for orders not yet acknowledged / drafts.
fn route_line(f: &mut Frame, cam: &Camera, pts: &[[f64; 2]], c: Rgba, dashed: bool) {
    for w in pts.windows(2) {
        let (a, b) = (cam.to_screen(w[0]), cam.to_screen(w[1]));
        if !dashed {
            f.line(a, b, 1.5, c);
            continue;
        }
        let len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
        let n = (len / 8.0).ceil().max(1.0) as usize;
        for k in (0..n).step_by(2) {
            let t0 = k as f32 / n as f32;
            let t1 = ((k + 1) as f32 / n as f32).min(1.0);
            let p = |t: f32| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
            f.line(p(t0), p(t1), 1.5, c);
        }
    }
}

/// Checkpoints (numbered) and the destination of a route.
fn checkpoints(f: &mut Frame, cam: &Camera, pts: &[[f64; 2]], c: Rgba) {
    let Some((last, cps)) = pts.split_last() else { return };
    for (i, p) in cps.iter().enumerate() {
        // Dark disc behind the number so it reads over any map layer.
        let s = cam.to_screen(*p);
        f.circle(s, 10.0, style::PANEL);
        f.circle_lines(s, 10.0, c);
        let t = format!("{}", i + 1);
        let tw = f.text_width(&t, style::FONT_S);
        f.text(&t, s[0] - tw / 2.0, s[1] - style::FONT_S as f32 / 2.0, style::FONT_S, c);
        let cw = f.text_width("CP", style::FONT_S);
        f.rect(s[0] + 11.0, s[1] - 8.0, cw + 6.0, 16.0, style::PANEL);
        f.text("CP", s[0] + 14.0, s[1] - style::FONT_S as f32 / 2.0, style::FONT_S, c);
    }
    let s = cam.to_screen(*last);
    f.line([s[0] - 6.0, s[1] - 6.0], [s[0] + 6.0, s[1] + 6.0], 1.5, c);
    f.line([s[0] - 6.0, s[1] + 6.0], [s[0] + 6.0, s[1] - 6.0], 1.5, c);
}

fn speed_short(v: Option<f64>) -> String {
    v.map_or("MAX".into(), |v| format!("{} KM/H", format::kmh(v)))
}

/// Radio suffix for an ordered march speed.
fn speed_radio(v: Option<f64>) -> String {
    v.map_or(String::new(), |v| format!(", SPEED {}", format::kmh(v)))
}

fn short_task(u: &sim::view::OwnUnit) -> String {
    let t = match &u.task {
        Task::Idle if u.held => "HALTED, ROUTE HELD".to_owned(),
        Task::Idle => "HALTED".to_owned(),
        Task::Moving { remaining_m } => format!("MOVING {}", format::distance(*remaining_m)),
        Task::Set => u.position_note.clone().unwrap_or("SET".into()),
        Task::Engaging => "ENGAGING".into(),
        Task::Overwatch => "OVERWATCH".into(),
        Task::Occupying => "OCCUPYING".into(),
    };
    match u.condition {
        Condition::Operational => t,
        Condition::MobilityKill => "M-KILL".into(),
        Condition::FirepowerKill => "F-KILL".into(),
        Condition::Destroyed => "DESTROYED".into(),
    }
}

fn short_technique(t: Technique) -> &'static str {
    match t {
        Technique::Traveling => "TRAVEL",
        Technique::TravelingOverwatch => "TRAV OVW",
        Technique::BoundingOverwatch => "BOUND OVW",
    }
}

fn weapons_str(w: WeaponsStatus) -> &'static str {
    match w {
        WeaponsStatus::Free => "FREE",
        WeaponsStatus::Tight => "TIGHT",
        WeaponsStatus::Hold => "HOLD",
    }
}

fn condition_colour(c: Condition) -> Rgba {
    match c {
        Condition::Operational => style::TEXT,
        Condition::Destroyed => style::HOSTILE,
        _ => style::AMBER,
    }
}

pub fn format_formation(f: Formation) -> String {
    format!("{f:?}").to_uppercase()
}

pub fn format_technique(t: Technique) -> &'static str {
    match t {
        Technique::Traveling => "TRAVELING",
        Technique::TravelingOverwatch => "TRAVELING OVERWATCH",
        Technique::BoundingOverwatch => "BOUNDING OVERWATCH",
    }
}

fn kill_text(k: KillKind) -> &'static str {
    match k {
        KillKind::Catastrophic => "DESTROYED",
        KillKind::Firepower => "FIREPOWER KILL",
        KillKind::Mobility => "MOBILITY KILL",
        KillKind::Crew => "CREW CASUALTIES",
    }
}

/// Terse radio text for an event (callsign, grid, distance; no flavour).
pub fn radio_text(e: &Event, pic: &PerceivedWorld, game: &Game) -> Option<String> {
    let unit = |id: EntityId| pic.unit(id);
    Some(match e {
        Event::RoutePlanned { unit: u, length_m } => {
            let u = unit(*u)?;
            let dest = u.route.last().map_or(format::grid(u.pos), |&p| format::grid(p));
            format!("{}: MOVING TO GRID {dest}, {}.", u.callsign, format::distance(*length_m))
        }
        Event::NoRoute { unit: u } => format!("{}: UNABLE. NO ROUTE.", unit(*u)?.callsign),
        Event::Arrived { unit: u } => {
            let u = unit(*u)?;
            format!("{}: SET AT GRID {}.", u.callsign, format::grid(u.pos))
        }
        Event::Halted { unit: u } => {
            let u = unit(*u)?;
            if u.platoon.is_some() {
                return None;
            }
            format!("{}: HALTED AT GRID {}.", u.callsign, format::grid(u.pos))
        }
        Event::Stuck { unit: u } => {
            let u = unit(*u)?;
            format!("{}: IMMOBILE AT GRID {}.", u.callsign, format::grid(u.pos))
        }
        Event::OutOfFuel { unit: u } => {
            let u = unit(*u)?;
            format!("{}: BLACK ON FUEL AT GRID {}.", u.callsign, format::grid(u.pos))
        }
        Event::OrderReceived { side, to } if *side == pic.side => match to {
            Addressee::Platoon(p) => format!("{}: ROGER, OUT.", game.platoon_cs(*p)),
            Addressee::Unit(u) => format!("{}: ROGER.", unit(*u)?.callsign),
        },
        Event::SpotReport { side, reporter, contact, level, new, range_m } if *side == pic.side => {
            let c = pic.contact(*contact)?;
            let r = unit(*reporter)?;
            let what = match (&c.type_name, level) {
                (Some(t), _) => t.clone(),
                (None, ContactLevel::Recognised) => "TANK".into(),
                (None, _) => "UNKNOWN VEHICLE".into(),
            };
            let dir = format::mils(ai::drive::bearing(r.pos, c.pos));
            let head = if *new { "CONTACT" } else { "UPDATE" };
            format!(
                "{}: {head}, {what}, GRID {}, DIRECTION {dir}, {}.",
                r.callsign,
                format::grid(c.pos),
                format::distance(*range_m)
            )
        }
        Event::ShotResult { unit: u, kill: Some(k), .. } => {
            let u = unit(*u)?;
            match k {
                KillKind::Catastrophic => format!("{}: TARGET DESTROYED.", u.callsign),
                _ => format!("{}: TARGET HIT.", u.callsign),
            }
        }
        Event::Hit { unit: u, penetrated, kill } => {
            let u = unit(*u)?;
            match (kill, penetrated) {
                (Some(KillKind::Catastrophic), _) => {
                    format!("{} IS DESTROYED AT GRID {}.", u.callsign, format::grid(u.pos))
                }
                (Some(k), _) => format!("{}: HIT. {}.", u.callsign, kill_text(*k)),
                (None, false) => format!("{}: HIT, NO PENETRATION.", u.callsign),
                (None, true) => format!("{}: HIT, MINOR DAMAGE.", u.callsign),
            }
        }
        Event::TriggerFired { side, id } if *side == pic.side => {
            let t = pic.triggers.iter().find(|t| t.id == *id)?;
            format!("CODE WORD {}: EXECUTE {}.", id, t.order)
        }
        Event::PlatoonReport { side, platoon, report } if *side == pic.side => {
            let cs = game.platoon_cs(*platoon);
            match report {
                PlatoonReport::SetAt { pos } => format!("{cs}: SET AT GRID {}.", format::grid(*pos)),
                PlatoonReport::NoRoute => format!("{cs}: UNABLE. NO ROUTE."),
                PlatoonReport::SetInPosition { hull_down, turret_down, exposed, facing } => format!(
                    "{cs}: SET IN BP. {hull_down} HULL-DOWN, {turret_down} TURRET-DOWN, {exposed} EXPOSED. ORIENTED {} MILS.",
                    format::mils(*facing)
                ),
                PlatoonReport::Contact { at, toward } => format!(
                    "{cs}: CONTACT, DIRECTION {}, {}. ACTION, ENGAGING.",
                    format::mils(ai::drive::bearing(*at, *toward)),
                    format::distance(((toward[0] - at[0]).powi(2) + (toward[1] - at[1]).powi(2)).sqrt())
                ),
                PlatoonReport::ContinuingMission => format!("{cs}: CONTACT LOST. CONTINUING MISSION."),
                PlatoonReport::Bounding { section } => format!("{cs}: SECTION {section} BOUNDING."),
                PlatoonReport::Strength { remaining, of } => format!("{cs}: {remaining} OF {of} VEHICLES OPERATIONAL."),
            }
        }
        Event::ScenarioEnd { winner, reason } => {
            let names = game.sim.side_names();
            let name = |s: SideId| names.get(s.0 as usize).cloned().unwrap_or_default();
            let why = match reason {
                EndReason::ForceDestroyed { side } => format!("{} COMBAT INEFFECTIVE", name(*side)),
                EndReason::ObjectiveSeized { side } => format!("OBJECTIVE SEIZED BY {}", name(*side)),
                EndReason::TimeLimit { holder: Some(h) } => format!("TIME. OBJECTIVE HELD BY {}", name(*h)),
                EndReason::TimeLimit { holder: None } => "TIME. OBJECTIVE CONTESTED".to_owned(),
            };
            format!("ENDEX. {why}. {} WINS.", winner.map_or("NO SIDE".into(), name))
        }
        _ => return None,
    })
}
