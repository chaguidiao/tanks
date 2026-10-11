//! Game mode: runs the simulation, shows the side's perceived picture, issues orders.
//!
//! Only `PerceivedWorld` is read for display, selection and orders (CLAUDE.md rule 3).
//! Ground truth is drawn only with the `debug-truth` feature.

use std::collections::VecDeque;
use std::sync::Arc;

use sim::view::{Condition, ContactLevel, PerceivedWorld, PlatoonId, Task, WeaponsStatus};
use sim::{Addressee, Command, EntityId, Event, KillKind, PlatoonReport, PositionKind, SideId, Sim};
use sim_data::{DataSet, Formation, ScenarioDef, Technique};
use terrain::BakedMap;
use terrain::ground::Ground;

use crate::format;
use crate::render::{Frame, Input, Key, P2, Rgba};
use crate::style;
use crate::symbols;
use crate::view::Camera;

/// Time compression steps (DESIGN.md §2.1).
pub const SPEEDS: [u32; 4] = [1, 2, 4, 8];
const MAX_TICKS_PER_FRAME: u32 = 40;
/// Zoomed out beyond this (m/px), platoons are drawn as one symbol (DESIGN.md §10.1).
const AGGREGATE_MPP: f64 = 7.0;

#[derive(Debug, Clone, PartialEq)]
pub enum Tool {
    None,
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
}

/// A graphic the player drew with an order (dashed until acknowledged).
#[derive(Debug, Clone)]
pub struct OrderGraphic {
    pub platoon: Option<PlatoonId>,
    pub kind: GraphicKind,
    pub acknowledged: bool,
}

#[derive(Debug, Clone)]
pub enum GraphicKind {
    Bp { centre: [f64; 2], radius: f64, facing: f64, label: String },
}

/// Side effects for the app (textures etc.).
#[derive(Default)]
pub struct GameFx {
    /// Viewshed overlay: (rgba, w, h, origin x, origin y, step m).
    pub viewshed: Option<(Vec<u8>, usize, usize, f64, f64, f64)>,
    pub clear_overlay: bool,
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
    pub tool: Tool,
    pub bp_radius: f64,
    pub graphics: Vec<OrderGraphic>,
    acc: f64,
    prev: Vec<(EntityId, [f64; 2], f64)>,
    pub log: VecDeque<(String, String)>,
    bp_count: u32,
}

impl Game {
    pub fn new(scenario: ScenarioDef, ds: Arc<DataSet>, map: Arc<BakedMap>) -> Result<Self, String> {
        let sim = Sim::new(&scenario, ds, map)?;
        let mut g = Self {
            sim,
            scenario,
            side: SideId(0),
            sel_platoon: None,
            sel_unit: None,
            paused: true,
            speed: 0,
            technique: Technique::Traveling,
            tool: Tool::None,
            bp_radius: 250.0,
            graphics: Vec::new(),
            acc: 0.0,
            prev: Vec::new(),
            log: VecDeque::new(),
            bp_count: 0,
        };
        if let Some((id, p)) = g.picture().own.first().map(|u| (u.id, u.platoon)) {
            g.sel_unit = Some(id);
            g.sel_platoon = p;
        }
        g.capture_prev();
        let name = g.scenario.name.clone();
        g.radio(format!("{name}. PAUSED. SPACE TO START. F7 HELP."));
        Ok(g)
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
        self.sim.submit(self.side, Command::Move { unit, destination: dest, speed_limit_mps: None });
    }

    pub fn advance_ticks(&mut self, n: u32) {
        for _ in 0..n {
            self.capture_prev();
            let events = self.sim.step();
            for e in events {
                self.on_event(&e);
            }
        }
    }

    fn on_event(&mut self, e: &Event) {
        if let Event::OrderReceived { side, to: Addressee::Platoon(p) } = e
            && *side == self.side
        {
            for g in self.graphics.iter_mut().filter(|g| g.platoon == Some(*p)) {
                g.acknowledged = true;
            }
        }
        if let Some(t) = radio_text(e, self.sim.perceived(self.side), self) {
            self.radio(t);
        }
    }

    pub fn update(&mut self, inp: &Input, cam: &Camera, over_ui: bool) -> GameFx {
        let mut fx = GameFx::default();
        if inp.hit(Key::Space) {
            self.paused = !self.paused;
        }
        for (k, _) in SPEEDS.iter().enumerate() {
            if inp.hit(Key::Num(k as u8 + 1)) {
                self.speed = k;
            }
        }
        if inp.hit(Key::Tab) {
            let ps: Vec<(PlatoonId, Option<EntityId>)> =
                self.picture().platoons.iter().map(|p| (p.id, p.members.first().copied())).collect();
            if !ps.is_empty() {
                let i =
                    self.sel_platoon.and_then(|s| ps.iter().position(|p| p.0 == s)).map_or(0, |i| (i + 1) % ps.len());
                self.sel_platoon = Some(ps[i].0);
                self.sel_unit = ps[i].1;
            }
        }
        if inp.hit(Key::Escape) {
            if self.tool != Tool::None {
                if self.tool == Tool::Viewshed {
                    fx.clear_overlay = true;
                }
                self.tool = Tool::None;
            } else {
                self.sel_platoon = None;
                self.sel_unit = None;
            }
        }
        if inp.hit(Key::Char('v')) {
            self.tool = if self.tool == Tool::Viewshed { Tool::None } else { Tool::Viewshed };
            fx.clear_overlay = true;
        }
        if inp.hit(Key::Char('l')) {
            self.tool = Tool::Los { a: None, b: None, result: None };
        }
        if inp.hit(Key::Char('b')) && self.sel_platoon.is_some() {
            self.tool = Tool::BattlePosition { centre: None };
        }
        if inp.hit(Key::LeftBracket) {
            self.bp_radius = (self.bp_radius / 1.25).max(100.0);
        }
        if inp.hit(Key::RightBracket) {
            self.bp_radius = (self.bp_radius * 1.25).min(800.0);
        }
        if inp.hit(Key::Char('t')) {
            self.technique = match self.technique {
                Technique::Traveling => Technique::TravelingOverwatch,
                Technique::TravelingOverwatch => Technique::BoundingOverwatch,
                Technique::BoundingOverwatch => Technique::Traveling,
            };
        }
        if inp.hit(Key::Char('f'))
            && let Some(p) = self.sel_platoon
            && let Some(v) = self.picture().platoon(p)
        {
            let i = Formation::ALL.iter().position(|f| *f == v.formation).unwrap_or(0);
            let f = Formation::ALL[(i + 1) % Formation::ALL.len()];
            self.sim.submit(self.side, Command::SetFormation { platoon: p, formation: f });
            let cs = self.platoon_cs(p);
            self.radio(format!("TO {cs}: CHANGE FORMATION, {}.", format_formation(f)));
        }
        if inp.hit(Key::Char('w'))
            && let Some(to) = self.addressee()
        {
            let cur = match to {
                Addressee::Unit(u) => self.picture().unit(u).map(|u| u.weapons),
                Addressee::Platoon(p) => self
                    .picture()
                    .platoon(p)
                    .and_then(|v| v.members.first())
                    .and_then(|u| self.picture().unit(*u))
                    .map(|u| u.weapons),
            }
            .unwrap_or(WeaponsStatus::Free);
            let next = match cur {
                WeaponsStatus::Free => WeaponsStatus::Tight,
                WeaponsStatus::Tight => WeaponsStatus::Hold,
                WeaponsStatus::Hold => WeaponsStatus::Free,
            };
            self.sim.submit(self.side, Command::WeaponsControl { to, status: next });
            self.radio(format!("WEAPONS {}.", format!("{next:?}").to_uppercase()));
        }
        if inp.hit(Key::Char('h'))
            && let Some(to) = self.addressee()
        {
            self.sim.submit(self.side, Command::Halt { to });
        }

        let world = cam.to_world(inp.mouse);
        if !over_ui && inp.left_pressed {
            match self.tool.clone() {
                Tool::None => self.select(cam, inp.mouse, inp.ctrl),
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
            }
        }
        if !over_ui && inp.right_pressed {
            if inp.ctrl {
                self.engage_at(cam, inp.mouse);
            } else {
                match (self.sel_platoon, self.sel_unit) {
                    (Some(p), _) => {
                        self.sim.submit(
                            self.side,
                            Command::PlatoonMove {
                                platoon: p,
                                destination: world,
                                formation: None,
                                technique: self.technique,
                            },
                        );
                        let cs = self.platoon_cs(p);
                        self.radio(format!(
                            "TO {cs}: MOVE TO GRID {}, {}.",
                            format::grid(world),
                            format_technique(self.technique)
                        ));
                        self.graphics.retain(|g| g.platoon != Some(p));
                    }
                    (None, Some(u)) => self.order_move(u, world),
                    _ => {}
                }
            }
        }
        if !self.paused {
            self.acc += inp.frame_time as f64 * SPEEDS[self.speed] as f64;
            let n = ((self.acc / sim::DT) as u32).min(MAX_TICKS_PER_FRAME);
            self.acc -= n as f64 * sim::DT;
            self.acc = self.acc.min(sim::DT * 2.0);
            self.advance_ticks(n);
        }
        fx
    }

    pub fn issue_bp(&mut self, centre: [f64; 2], facing: f64) {
        let Some(p) = self.sel_platoon else { return };
        self.sim.submit(
            self.side,
            Command::OccupyPosition {
                platoon: p,
                centre,
                radius_m: self.bp_radius,
                facing,
                kind: PositionKind::HullDown,
            },
        );
        self.bp_count += 1;
        let label = format!("BP {}", self.bp_count);
        let cs = self.platoon_cs(p);
        self.radio(format!("TO {cs}: OCCUPY {label}, HULL-DOWN, ORIENT {} MILS.", format::mils(facing)));
        self.graphics.retain(|g| g.platoon != Some(p));
        self.graphics.push(OrderGraphic {
            platoon: Some(p),
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
            self.sim.submit(self.side, Command::Engage { unit: u, contact: cid });
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
        let pic = self.picture();
        for g in &self.graphics {
            match &g.kind {
                GraphicKind::Bp { centre, radius, facing, label } => {
                    symbols::battle_position(f, cam, *centre, *radius, *facing, label, g.acknowledged);
                }
            }
        }
        // Routes: solid while executing (DESIGN.md §10.3).
        for u in &pic.own {
            if u.route.len() >= 2
                && (self.sel_platoon.is_none() || u.platoon == self.sel_platoon || cam.mpp < AGGREGATE_MPP)
            {
                for w in u.route.windows(2) {
                    f.line(cam.to_screen(w[0]), cam.to_screen(w[1]), 1.0, style::ROUTE);
                }
                f.circle_lines(cam.to_screen(*u.route.last().unwrap()), 4.0, style::ROUTE);
            }
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
            Tool::BattlePosition { centre } => {
                let c = centre.unwrap_or_else(|| cam.to_world(mouse));
                let facing = centre.map_or(0.0, |c| ai::drive::bearing(c, cam.to_world(mouse)));
                symbols::battle_position(f, cam, c, self.bp_radius, facing, "BP", false);
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

    pub fn draw_ui(&self, f: &mut Frame, screen: P2) {
        let side = &self.sim.side_names()[self.side.0 as usize];
        let state = if self.paused { "PAUSED".to_owned() } else { format!("X{}", SPEEDS[self.speed]) };
        let top = format!("{}   {}   {}   {}", format::clock(self.sim.clock_s()), state, side, self.scenario.name);
        f.text(&top, 10.0, 6.0, style::FONT_L - 4, if self.paused { style::AMBER } else { style::TEXT });
        let tool = match &self.tool {
            Tool::None => String::new(),
            Tool::BattlePosition { centre: None } => "BP: CLICK CENTRE  [ ] RADIUS".into(),
            Tool::BattlePosition { .. } => "BP: CLICK THREAT DIRECTION".into(),
            Tool::Viewshed => "VIEWSHED: CLICK OBSERVER POINT  (V/ESC CLOSE)".into(),
            Tool::Los { .. } => "LOS: CLICK TWO POINTS".into(),
        };
        f.text(&tool, 560.0, 8.0, style::FONT_L - 8, style::AMBER);

        // Unit / platoon panel.
        let pw = 300.0;
        let (px, py) = (screen[0] - pw - 10.0, 40.0);
        let ph = 230.0;
        f.rect(px, py, pw, ph, style::PANEL);
        f.rect_lines(px, py, pw, ph, style::PANEL_EDGE);
        let pic = self.picture();
        let mut lines: Vec<(String, Rgba)> = Vec::new();
        if let Some(pv) = self.sel_platoon.and_then(|p| pic.platoon(p)) {
            lines.push((format!("PLATOON {}", pv.callsign), style::FRIENDLY));
            lines.push((pv.task.clone(), style::TEXT));
            lines.push((
                format!("FORM {}  TECH {}", format_formation(pv.formation), format_technique(self.technique)),
                style::TEXT_DIM,
            ));
            for m in pv.members.iter().filter_map(|m| pic.unit(*m)) {
                lines.push((unit_line(m), condition_colour(m.condition)));
            }
        } else if let Some(u) = self.sel_unit.and_then(|id| pic.unit(id)) {
            lines.push((format!("{}  {}", u.callsign, u.vehicle_name), style::FRIENDLY));
            lines.push((format!("GRID {}  HDG {}", format::grid8(u.pos), format::mils(u.heading)), style::TEXT));
            lines.push((unit_line(u), condition_colour(u.condition)));
            lines.push((format!("FUEL {:.0} %", u.fuel_fraction * 100.0), style::TEXT));
        } else {
            lines.push(("NO UNIT SELECTED".into(), style::TEXT_DIM));
        }
        let contacts = pic.live_contacts().count();
        lines.push((
            format!("CONTACTS {contacts}   KILLS {}", pic.contacts.iter().filter(|c| c.destroyed).count()),
            style::TEXT_DIM,
        ));
        for (k, (l, c)) in lines.iter().take(11).enumerate() {
            f.text(l, px + 10.0, py + 8.0 + k as f32 * 19.0, style::FONT_L - 8, *c);
        }

        // Radio log.
        let n = 8;
        let (lw, lh) = (640.0, n as f32 * 16.0 + 12.0);
        let (lx, ly) = (10.0, screen[1] - lh - 28.0);
        f.rect(lx, ly, lw, lh, style::PANEL);
        f.rect_lines(lx, ly, lw, lh, style::PANEL_EDGE);
        for (k, (t, msg)) in self.log.iter().rev().take(n).collect::<Vec<_>>().into_iter().rev().enumerate() {
            f.text(&format!("{t}  {msg}"), lx + 8.0, ly + 6.0 + k as f32 * 16.0, style::FONT_L - 8, style::TEXT);
        }
    }

    pub fn over_ui(&self, s: P2, screen: P2) -> bool {
        let in_panel = s[0] > screen[0] - 310.0 && s[1] > 40.0 && s[1] < 270.0;
        let in_log = s[0] < 650.0 && s[1] > screen[1] - 8.0 * 16.0 - 40.0 && s[1] < screen[1] - 28.0;
        in_panel || in_log
    }
}

fn unit_line(u: &sim::view::OwnUnit) -> String {
    let task = match &u.task {
        Task::Idle => "HALTED".to_owned(),
        Task::Moving { remaining_m } => format!("MOVING {}", format::distance(*remaining_m)),
        Task::Set => u.position_note.clone().unwrap_or("SET".into()),
        Task::Engaging => "ENGAGING".into(),
        Task::Overwatch => "OVERWATCH".into(),
        Task::Occupying => "OCCUPYING".into(),
    };
    let cond = match u.condition {
        Condition::Operational => "",
        Condition::MobilityKill => " M-KILL",
        Condition::FirepowerKill => " F-KILL",
        Condition::Destroyed => " DESTROYED",
    };
    format!(
        "{:<4} {:>3} KM/H  {:>2} RDS  {} {}{}",
        u.callsign,
        format::kmh(u.speed),
        u.ammo,
        weapons_str(u.weapons),
        task,
        cond
    )
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
        Event::ScenarioEnd { reason, .. } => format!("ENDEX. {reason}"),
        _ => return None,
    })
}
