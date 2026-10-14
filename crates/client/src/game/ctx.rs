//! Right-click handling and the map context menu (docs/plans/PLANS-AND-CONTEXT-MENU.md §4).
//!
//! A plain right click still moves the selection (on release); holding it, or clicking an
//! object, opens a menu whose items dispatch the same actions as the command card.

use ai::picture::{ContactId, ContactReaction};

use super::planedit::PlanEdit;
use super::posture::{Posture, PostureVal, Scope};
use super::*;
use crate::hud::{MenuEvent, MenuItem, PopupMenu};

/// Hold time that turns a right click into a menu request (s) and the drift allowed (px).
const HOLD_S: f32 = 0.25;
const DRIFT_PX: f32 = 4.0;

/// A right button press waiting for release or the hold time.
#[derive(Debug, Clone, Copy)]
pub struct Rmb {
    at: P2,
    held: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MenuTarget {
    Ground,
    Unit(EntityId),
    Contact(ContactId),
    Graphic(u32),
    /// A checkpoint of the shown plan (draft or executing).
    PlanCp(u16),
    /// The leg that ends at checkpoint `id`.
    PlanLeg(u16),
}

#[derive(Debug, Clone, PartialEq)]
pub enum MenuAct {
    MoveHere(Option<f64>),
    RouteVia,
    BattlePositionHere,
    SmokeRoundAt,
    CallFireAt(FireKind),
    Draw(DrawKind),
    LosFrom,
    ViewshedHere,
    NextOrder(OrderMode),
    Cmd(Cmd),
    SelectPlatoon(PlatoonId),
    SelectUnit(EntityId),
    VehicleData,
    React(PlatoonId, ContactReaction),
    Engage(ContactId),
    LosTo,
    RemoveGraphic(u32),
    Plan(PlanEdit),
    Posture(Scope, PostureVal),
}

pub(super) fn reaction_name(r: ContactReaction) -> &'static str {
    match r {
        ContactReaction::Drill => "DRILL (HALT)",
        ContactReaction::Continue => "CONTINUE",
        ContactReaction::HastyPosition => "HASTY POSITION",
        ContactReaction::BreakContact => "BREAK CONTACT",
    }
}

impl Game {
    pub fn menu(&self) -> Option<&PopupMenu<MenuAct>> {
        self.menu.as_ref()
    }

    pub fn menu_frames(&self, screen: P2) -> Vec<hud::Rect> {
        self.menu.as_ref().map_or_else(Vec::new, |m| m.frames(screen))
    }

    /// What is under the cursor: plan checkpoint, own unit, contact, graphic, plan leg, else the
    /// ground.
    /// While the right button is held on the map: where, and how far through the hold time.
    pub(super) fn rmb_hold(&self) -> Option<(P2, f32)> {
        self.rmb.filter(|r| r.held > 0.0).map(|r| (r.at, (r.held / HOLD_S).min(1.0)))
    }

    pub(super) fn menu_target(&self, cam: &Camera, s: P2) -> MenuTarget {
        if let Some(id) = self.plan_cp_at(cam, s) {
            return MenuTarget::PlanCp(id);
        }
        let near = |p: [f64; 2], r: f32| {
            let q = cam.to_screen(p);
            (q[0] - s[0]).powi(2) + (q[1] - s[1]).powi(2) < r * r
        };
        let pic = self.picture();
        if let Some(u) = pic.own.iter().find(|u| near(u.pos, 18.0)) {
            return MenuTarget::Unit(u.id);
        }
        if let Some(c) = pic.live_contacts().find(|c| near(c.pos, 20.0)) {
            return MenuTarget::Contact(c.id);
        }
        for g in &pic.graphics {
            let hit = match &g.kind {
                SimGraphic::Trp { at } => near(*at, 14.0),
                SimGraphic::Objective { centre, .. } | SimGraphic::BattlePosition { centre, .. } => near(*centre, 16.0),
                SimGraphic::PhaseLine { points } => points.windows(2).any(|w| {
                    let (a, b) = (cam.to_screen(w[0]), cam.to_screen(w[1]));
                    seg_dist(s, a, b) < 6.0
                }),
            };
            if hit {
                return MenuTarget::Graphic(g.id);
            }
        }
        if let Some(id) = self.plan_leg_at(cam, s) {
            return MenuTarget::PlanLeg(id);
        }
        MenuTarget::Ground
    }

    pub fn open_menu(&mut self, cam: &Camera, at: P2) {
        let world = cam.to_world(at);
        let target = self.menu_target(cam, at);
        let (title, items) = self.build_menu(target, world);
        self.menu = Some(PopupMenu::new(at, title, items));
        self.menu_world = world;
        self.menu_target_id = target;
    }

    fn speed_label(v: Option<f64>) -> String {
        v.map_or("MAX".into(), |v| format!("{} KM/H", format::kmh(v)))
    }

    fn orders_submenu(&self) -> Vec<MenuItem<MenuAct>> {
        let sel = self.addressee().is_some();
        let item = |label: &str, key: &'static str, c: Cmd, ok: bool| {
            let it = MenuItem::new(label, MenuAct::Cmd(c)).key(key);
            if ok { it } else { it.disabled("NO UNIT SELECTED") }
        };
        let halt = if self.addressee().is_some_and(|to| self.shown_held(to)) { "RESUME" } else { "HALT" };
        vec![
            item(halt, "H", Cmd::Halt, sel),
            self.posture_item(Posture::Formation, "F"),
            self.posture_item(Posture::Technique, "T"),
            self.posture_item(Posture::Speed, "S"),
            self.posture_item(Posture::Weapons, "W"),
            item("SMOKE SCREEN", "Z", Cmd::Smoke, sel),
            item("ENGINE SMOKE", "E", Cmd::EngineSmoke, sel),
            item("REVERSE 100 M", "D", Cmd::Reverse, sel),
            self.posture_item(Posture::Ammo, "Q"),
            item("BREAK CONTACT", "X", Cmd::BreakContact, self.sel_platoon.is_some()),
        ]
    }

    fn fire_item(&self, label: &str) -> MenuItem<MenuAct> {
        let it = MenuItem::submenu(label, self.fire_submenu());
        if self.picture().fire_support.is_empty() { it.disabled("NO BATTERY") } else { it }
    }

    fn fire_submenu(&self) -> Vec<MenuItem<MenuAct>> {
        if self.picture().fire_support.is_empty() {
            return vec![MenuItem::new("HE", MenuAct::CallFireAt(FireKind::He)).disabled("NO BATTERY")];
        }
        let b = self.fire_battery();
        let he = b.is_some_and(|b| b.he_rounds > 0);
        let sm = b.is_some_and(|b| b.smoke_rounds > 0);
        let a = MenuItem::new("HE FIRE FOR EFFECT", MenuAct::CallFireAt(FireKind::He));
        let b = MenuItem::new("SMOKE", MenuAct::CallFireAt(FireKind::Smoke));
        vec![if he { a } else { a.disabled("NO HE ROUNDS") }, if sm { b } else { b.disabled("NO SMOKE ROUNDS") }]
    }

    pub(super) fn build_menu(&self, target: MenuTarget, world: [f64; 2]) -> (String, Vec<MenuItem<MenuAct>>) {
        let sel = self.addressee();
        let no_unit = "NO UNIT SELECTED";
        let grid = format::grid(world);
        match target {
            MenuTarget::Ground => {
                let cur_speed = self.current_speed();
                let mut speeds = Vec::new();
                for s in self.speed_presets() {
                    speeds.push(MenuItem::new(Self::speed_label(s), MenuAct::MoveHere(s)).checked(s == cur_speed));
                }
                let mut v = Vec::new();
                let mv = MenuItem::new("MOVE HERE", MenuAct::MoveHere(cur_speed))
                    .key("M")
                    .value(Self::speed_label(cur_speed));
                v.push(if sel.is_some() { mv } else { mv.disabled(no_unit) });
                let mut at = MenuItem::submenu("MOVE HERE AT", speeds);
                if sel.is_none() {
                    at = at.disabled(no_unit);
                }
                v.push(at);
                let r = MenuItem::new("ROUTE VIA HERE", MenuAct::RouteVia).key("R");
                v.push(if sel.is_some() { r } else { r.disabled(no_unit) });
                let cp = MenuItem::new("ADD CP TO PLAN", MenuAct::Plan(PlanEdit::AddCp)).key("P");
                v.push(if self.sel_platoon.is_some() { cp } else { cp.disabled("NO PLATOON SELECTED") });
                let bp = MenuItem::new("BATTLE POSITION HERE", MenuAct::BattlePositionHere).key("B");
                v.push(if self.sel_platoon.is_some() { bp } else { bp.disabled("NO PLATOON SELECTED") });
                let sm = MenuItem::new("SMOKE ROUNDS HERE", MenuAct::SmokeRoundAt).key("N");
                v.push(if self.smoke_rounds(sel) > 0 { sm } else { sm.disabled("NO SMOKE ROUNDS") });
                v.push(self.fire_item("CALL FOR FIRE").key("Y"));
                v.push(MenuItem::submenu(
                    "DRAW",
                    vec![
                        MenuItem::new("PHASE LINE FROM HERE", MenuAct::Draw(DrawKind::PhaseLine)),
                        MenuItem::new("OBJECTIVE CENTRED HERE", MenuAct::Draw(DrawKind::Objective)),
                        MenuItem::new("TARGET REFERENCE POINT HERE", MenuAct::Draw(DrawKind::Trp)),
                    ],
                ));
                v.push(MenuItem::new("LOS FROM HERE", MenuAct::LosFrom).key("L"));
                v.push(MenuItem::new("VIEWSHED HERE", MenuAct::ViewshedHere).key("V"));
                v.push(MenuItem::submenu(
                    "NEXT ORDER",
                    [
                        ("NOW", OrderMode::Now),
                        ("ON ENEMY CROSSING", OrderMode::OnEnemyCrossing),
                        ("IN 5 MIN", OrderMode::InFiveMinutes),
                    ]
                    .into_iter()
                    .map(|(l, m)| MenuItem::new(l, MenuAct::NextOrder(m)).checked(self.order_mode == m))
                    .collect(),
                ));
                v.push(MenuItem::submenu("ORDERS", self.orders_submenu()));
                (format!("GRID {grid}"), v)
            }
            MenuTarget::Unit(id) => {
                let Some(u) = self.picture().unit(id) else { return (String::new(), Vec::new()) };
                let mut v = Vec::new();
                if let Some(p) = u.platoon {
                    v.push(
                        MenuItem::new("SELECT PLATOON", MenuAct::SelectPlatoon(p)).checked(self.sel_platoon == Some(p)),
                    );
                }
                v.push(MenuItem::new("SELECT VEHICLE", MenuAct::SelectUnit(id)));
                v.push(MenuItem::new("DATA SHEET", MenuAct::VehicleData).key("I"));
                if let Some(p) = u.platoon {
                    v.push(MenuItem::new("EDIT PLAN", MenuAct::Plan(PlanEdit::Edit(p))).key("P"));
                    if self.picture().platoon(p).is_some_and(|v| v.plan.is_some()) && self.sel_platoon == Some(p) {
                        v.push(MenuItem::new("CLEAR PLAN", MenuAct::Plan(PlanEdit::Clear)));
                    }
                }
                if let Some(p) = u.platoon
                    && let Some(view) = self.picture().platoon(p)
                {
                    v.push(MenuItem::submenu(
                        "ON CONTACT",
                        [
                            ContactReaction::Drill,
                            ContactReaction::Continue,
                            ContactReaction::HastyPosition,
                            ContactReaction::BreakContact,
                        ]
                        .into_iter()
                        .map(|r| {
                            MenuItem::new(reaction_name(r), MenuAct::React(p, r)).checked(view.contact_reaction == r)
                        })
                        .collect(),
                    ));
                }
                v.push(MenuItem::submenu("ORDERS", self.orders_submenu()));
                (u.callsign.clone(), v)
            }
            MenuTarget::Contact(id) => {
                let name = self.picture().contact(id).map_or("CONTACT".into(), |c| {
                    c.type_name.clone().unwrap_or_else(|| "CONTACT".into()).to_uppercase()
                });
                let eng = MenuItem::new("ENGAGE", MenuAct::Engage(id)).key("A");
                let sm = MenuItem::new("SMOKE ROUNDS ON", MenuAct::SmokeRoundAt).key("N");
                let v = vec![
                    if self.sel_unit.is_some() { eng } else { eng.disabled("NO VEHICLE SELECTED") },
                    self.fire_item("CALL FOR FIRE ON"),
                    if self.smoke_rounds(sel) > 0 { sm } else { sm.disabled("NO SMOKE ROUNDS") },
                    {
                        let l = MenuItem::new("LOS TO", MenuAct::LosTo);
                        if self.selection_centre().is_some() { l } else { l.disabled(no_unit) }
                    },
                ];
                (name, v)
            }
            MenuTarget::Graphic(id) => {
                let label = self.picture().graphic(id).map_or("GRAPHIC".into(), |g| g.label.clone());
                let mv = MenuItem::new("MOVE TO", MenuAct::MoveHere(self.current_speed()));
                let v = vec![
                    if sel.is_some() { mv } else { mv.disabled(no_unit) },
                    self.fire_item("CALL FOR FIRE ON"),
                    MenuItem::new("LOS FROM HERE", MenuAct::LosFrom),
                    MenuItem::new("DELETE", MenuAct::RemoveGraphic(id)),
                ];
                (label, v)
            }
            MenuTarget::PlanCp(id) => self.plan_cp_menu(id),
            MenuTarget::PlanLeg(id) => self.plan_leg_menu(id),
        }
    }

    /// The point a menu on `target` refers to (a contact or graphic centres on itself).
    fn menu_point(&self, at: [f64; 2]) -> [f64; 2] {
        match self.menu_target_id {
            MenuTarget::Contact(id) => self.picture().contact(id).map_or(at, |c| c.pos),
            MenuTarget::Graphic(id) => match self.picture().graphic(id).map(|g| &g.kind) {
                Some(SimGraphic::Trp { at }) => *at,
                Some(SimGraphic::Objective { centre, .. }) | Some(SimGraphic::BattlePosition { centre, .. }) => *centre,
                _ => at,
            },
            _ => at,
        }
    }

    pub fn run_menu_act(&mut self, a: MenuAct, at: [f64; 2], fx: &mut GameFx) {
        let at = self.menu_point(at);
        match a {
            MenuAct::MoveHere(speed) => {
                self.draft.clear();
                self.issue_move_at(Vec::new(), at, speed);
            }
            MenuAct::RouteVia => {
                self.tool = Tool::Route;
                self.draft.push(at);
            }
            MenuAct::BattlePositionHere => self.tool = Tool::BattlePosition { centre: Some(at) },
            MenuAct::SmokeRoundAt => self.fire_smoke_round(at),
            MenuAct::CallFireAt(k) => {
                self.fire_kind = k;
                self.call_for_fire(at);
            }
            MenuAct::Draw(DrawKind::Trp) => {
                self.counts[2] += 1;
                let label = format!("TRP {}", self.counts[2]);
                self.add_graphic(label, SimGraphic::Trp { at });
            }
            MenuAct::Draw(kind) => self.tool = Tool::Graphic { kind, points: vec![at] },
            MenuAct::LosFrom => self.tool = Tool::Los { a: Some(at), b: None, result: None },
            MenuAct::ViewshedHere => {
                self.tool = Tool::Viewshed;
                fx.viewshed = Some(self.viewshed(at));
            }
            MenuAct::NextOrder(m) => self.order_mode = m,
            MenuAct::Cmd(c) => self.command(c, fx),
            MenuAct::SelectPlatoon(p) => self.act(GameAction::SelectPlatoon(p), fx),
            MenuAct::SelectUnit(u) => self.act(GameAction::SelectUnit(u), fx),
            MenuAct::VehicleData => {
                if let MenuTarget::Unit(u) = self.menu_target_id {
                    self.sel_unit = Some(u);
                }
                self.act(GameAction::VehicleData, fx);
            }
            MenuAct::React(p, r) => {
                self.order(Command::SetContactReaction { platoon: p, reaction: r });
                let cs = self.platoon_cs(p);
                self.radio(format!("TO {cs}: ON CONTACT, {}.", reaction_name(r)));
            }
            MenuAct::Engage(c) => {
                if let Some(u) = self.sel_unit {
                    self.order(Command::Engage { unit: u, contact: c });
                    self.radio("ENGAGE THAT TARGET.".into());
                }
            }
            MenuAct::LosTo => {
                if let Some(from) = self.selection_centre() {
                    let r = self.los(from, at);
                    self.tool = Tool::Los { a: Some(from), b: Some(at), result: Some(r) };
                }
            }
            MenuAct::RemoveGraphic(id) => {
                self.sim.submit_immediate(self.side, Command::RemoveGraphic { id });
                self.sim.settle();
            }
            MenuAct::Plan(e) => self.run_plan_edit(e, at),
            MenuAct::Posture(scope, val) => self.apply_posture(scope, val),
        }
    }

    /// Run the open menu for one frame; true while it swallows the frame's map input.
    fn menu_update(&mut self, inp: &Input, fx: &mut GameFx) -> bool {
        let Some(m) = &mut self.menu else { return false };
        match m.update(inp) {
            MenuEvent::None => true,
            MenuEvent::Close => {
                self.menu = None;
                true
            }
            MenuEvent::Pick(a) => {
                self.menu = None;
                let at = self.menu_world;
                self.run_menu_act(a, at, fx);
                true
            }
        }
    }

    /// Right button handling for the plain map (no tool): menu on hold or on an object, else
    /// a move on release. Returns true if the frame's map click was consumed by the menu.
    pub(super) fn rmb_update(&mut self, inp: &Input, cam: &Camera, over_ui: bool, fx: &mut GameFx) -> bool {
        let swallowed = self.menu_update(inp, fx);
        if swallowed {
            self.rmb = None;
            return true;
        }
        if let Some(r) = &mut self.rmb {
            if inp.right_down {
                r.held += inp.frame_time;
                let drift = ((inp.mouse[0] - r.at[0]).powi(2) + (inp.mouse[1] - r.at[1]).powi(2)).sqrt();
                if r.held >= HOLD_S && drift <= DRIFT_PX {
                    let at = r.at;
                    self.rmb = None;
                    self.open_menu(cam, at);
                    return true;
                }
            }
            if inp.right_released {
                let r = self.rmb.take().expect("armed");
                if !over_ui {
                    let target = self.menu_target(cam, r.at);
                    if self.addressee().is_none() || target != MenuTarget::Ground {
                        self.open_menu(cam, r.at);
                    } else {
                        self.draft.clear();
                        self.issue_move(Vec::new(), cam.to_world(inp.mouse));
                    }
                }
            } else if !inp.right_down {
                self.rmb = None;
            }
        }
        false
    }

    /// A plain right press on the map arms the move/menu decision.
    pub(super) fn arm_rmb(&mut self, inp: &Input) {
        self.rmb = Some(Rmb { at: inp.mouse, held: 0.0 });
    }
}

pub(super) fn seg_dist(p: P2, a: P2, b: P2) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    let t = if l2 == 0.0 { 0.0 } else { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0) };
    ((p[0] - a[0] - t * dx).powi(2) + (p[1] - a[1] - t * dy).powi(2)).sqrt()
}
