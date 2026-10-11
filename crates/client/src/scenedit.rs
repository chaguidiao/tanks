//! Scenario editor (DESIGN.md §10.4): place and orient units, group them into platoons, set
//! objectives, date and start time, then save to `data/scenarios/<id>.ron` and play.

use sim_data::{DataSet, ObjectiveDef, ScenarioDef, SideDef, UnitDef};

use crate::format;
use crate::render::{Frame, Input, Key, P2};
use crate::style;
use crate::symbols;
use crate::view::Camera;

/// Pick radius for units and objectives (px).
const PICK_PX: f32 = 14.0;
/// Tanks per platoon (ATP 3-20.15 ch. 1: four-tank platoon).
const PLATOON_SIZE: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sel {
    Unit(usize, usize),
    Objective(usize),
}

#[derive(Debug, Default)]
pub struct Effects {
    pub save: bool,
    pub play: bool,
}

pub struct ScenarioEditor {
    pub def: ScenarioDef,
    side: usize,
    /// Vehicle id placed by P.
    vehicle: String,
    /// Platoon callsign placed into, per side.
    platoon: Vec<Option<String>>,
    sel: Option<Sel>,
    dragging: bool,
    pub unsaved: bool,
    pub status: String,
}

impl ScenarioEditor {
    pub fn new(mut def: ScenarioDef, ds: &DataSet) -> Self {
        while def.sides.len() < 2 {
            let (name, doctrine) =
                if def.sides.is_empty() { ("BLUE", "western-combined") } else { ("RED", "generic-opfor") };
            def.sides.push(SideDef {
                name: name.into(),
                doctrine: doctrine.into(),
                tech_year: None,
                units: Vec::new(),
                ai: None,
            });
        }
        let vehicle =
            def.sides[0].units.first().map(|u| u.vehicle.clone()).unwrap_or_else(|| {
                ds.vehicles_in_service(def.date.0).next().map_or_else(String::new, |v| v.id.clone())
            });
        let platoon = def.sides.iter().map(|s| s.units.last().and_then(|u| u.platoon.clone())).collect();
        Self { def, side: 0, vehicle, platoon, sel: None, dragging: false, unsaved: false, status: String::new() }
    }

    fn year(&self) -> i32 {
        self.def.sides[self.side].tech_year.unwrap_or(self.def.date.0)
    }

    fn pick(&self, cam: &Camera, s: P2) -> Option<Sel> {
        let d = |p: [f64; 2]| {
            let q = cam.to_screen(p);
            ((q[0] - s[0]).powi(2) + (q[1] - s[1]).powi(2)).sqrt()
        };
        let mut best: Option<(f32, Sel)> = None;
        for (si, side) in self.def.sides.iter().enumerate() {
            for (ui, u) in side.units.iter().enumerate() {
                let dist = d(u.position);
                if dist < PICK_PX && best.is_none_or(|(b, _)| dist < b) {
                    best = Some((dist, Sel::Unit(si, ui)));
                }
            }
        }
        if best.is_none() {
            for (k, o) in self.def.objectives.iter().enumerate() {
                if d(o.centre) < PICK_PX {
                    best = Some((0.0, Sel::Objective(k)));
                }
            }
        }
        best.map(|(_, s)| s)
    }

    /// Next free platoon number across both sides (platoon callsigns are numeric).
    fn new_platoon(&self) -> String {
        let max = self
            .def
            .sides
            .iter()
            .flat_map(|s| &s.units)
            .filter_map(|u| u.platoon.as_deref().and_then(|p| p.parse::<u32>().ok()))
            .max()
            .unwrap_or(0);
        (max + 1).to_string()
    }

    fn place(&mut self, at: [f64; 2]) {
        let side = self.side;
        let members = |p: &str| self.def.sides[side].units.iter().filter(|u| u.platoon.as_deref() == Some(p)).count();
        let platoon = match self.platoon[side].clone() {
            Some(p) if members(&p) < PLATOON_SIZE => p,
            _ => self.new_platoon(),
        };
        let n = members(&platoon) + 1;
        let mut callsign = format!("{platoon}-{n}");
        while self.def.sides.iter().flat_map(|s| &s.units).any(|u| u.callsign == callsign) {
            callsign.push('A');
        }
        let heading = self.def.sides[side].units.last().map_or(if side == 0 { 0.0 } else { 180.0 }, |u| u.heading_deg);
        self.def.sides[side].units.push(UnitDef {
            callsign: callsign.clone(),
            vehicle: self.vehicle.clone(),
            position: at,
            heading_deg: heading,
            platoon: Some(platoon.clone()),
        });
        self.platoon[side] = Some(platoon);
        self.sel = Some(Sel::Unit(side, self.def.sides[side].units.len() - 1));
        self.status = format!("PLACED {callsign}");
        self.unsaved = true;
    }

    pub fn update(&mut self, inp: &Input, cam: &Camera, ds: &DataSet, over_ui: bool) -> Effects {
        let mut fx = Effects::default();
        let m = cam.to_world(inp.mouse);

        if inp.left_pressed && !over_ui {
            self.sel = self.pick(cam, inp.mouse);
            self.dragging = self.sel.is_some();
        }
        if inp.left_released {
            self.dragging = false;
        }
        if self.dragging && inp.left_down && (inp.mouse_delta[0] != 0.0 || inp.mouse_delta[1] != 0.0) {
            match self.sel {
                Some(Sel::Unit(s, u)) => self.def.sides[s].units[u].position = m,
                Some(Sel::Objective(k)) => self.def.objectives[k].centre = m,
                None => {}
            }
            self.unsaved = true;
        }

        if inp.hit(Key::Char('p')) && !over_ui {
            self.place(m);
        }
        if inp.hit(Key::Char('n')) {
            self.platoon[self.side] = Some(self.new_platoon());
            self.status = format!("NEW PLATOON {}", self.platoon[self.side].as_deref().unwrap_or(""));
        }
        if inp.hit(Key::Char('o')) && !over_ui {
            let name = format!("OBJ {}", self.def.objectives.len() + 1);
            self.def.objectives.push(ObjectiveDef { name: name.clone(), centre: m, radius_m: 300.0 });
            self.sel = Some(Sel::Objective(self.def.objectives.len() - 1));
            self.status = format!("OBJECTIVE {name}");
            self.unsaved = true;
        }
        if inp.hit(Key::Delete) || inp.hit(Key::Backspace) {
            match self.sel.take() {
                Some(Sel::Unit(s, u)) => {
                    let gone = self.def.sides[s].units.remove(u);
                    self.status = format!("REMOVED {}", gone.callsign);
                }
                Some(Sel::Objective(k)) => {
                    let gone = self.def.objectives.remove(k);
                    self.status = format!("REMOVED {}", gone.name);
                }
                None => {}
            }
            self.unsaved = true;
        }
        if let Some(Sel::Objective(k)) = self.sel {
            let o = &mut self.def.objectives[k];
            if inp.hit(Key::LeftBracket) {
                o.radius_m = (o.radius_m - 50.0).max(100.0);
            }
            if inp.hit(Key::RightBracket) {
                o.radius_m = (o.radius_m + 50.0).min(1500.0);
            }
        }
        if inp.hit(Key::Char('r'))
            && !inp.ctrl
            && let Some(Sel::Unit(s, u)) = self.sel
        {
            let h = &mut self.def.sides[s].units[u].heading_deg;
            *h = (*h + 45.0).rem_euclid(360.0);
            self.unsaved = true;
        }
        if inp.hit(Key::Tab) {
            self.side = (self.side + 1) % self.def.sides.len();
            self.sel = None;
            self.status = format!("SIDE {}", self.def.sides[self.side].name);
        }
        if inp.hit(Key::Char('v')) {
            let list: Vec<&str> = ds.vehicles_in_service(self.year()).map(|v| v.id.as_str()).collect();
            if !list.is_empty() {
                let k = list.iter().position(|v| *v == self.vehicle).map_or(0, |k| (k + 1) % list.len());
                self.vehicle = list[k].to_owned();
                if let Some(Sel::Unit(s, u)) = self.sel {
                    self.def.sides[s].units[u].vehicle = self.vehicle.clone();
                    self.unsaved = true;
                }
            }
        }
        if inp.hit(Key::Char('y')) {
            self.def.date.0 -= 1;
            self.unsaved = true;
        }
        if inp.hit(Key::Char('u')) {
            self.def.date.0 += 1;
            self.unsaved = true;
        }
        if inp.hit(Key::Char('t')) {
            self.def.start_time_s = (self.def.start_time_s + 3 * 3600) % 86_400;
            self.unsaved = true;
        }
        if inp.ctrl && inp.hit(Key::Char('s')) {
            if !self.def.id.ends_with("-custom") {
                self.def.id.push_str("-custom");
                self.def.name.push_str(" (CUSTOM)");
            }
            fx.save = true;
        }
        if inp.hit(Key::F(9)) {
            fx.play = true;
        }
        fx
    }

    /// Problems that make the scenario unplayable (shown in the panel).
    fn problems(&self, ds: &DataSet) -> Vec<String> {
        let mut out = Vec::new();
        for s in &self.def.sides {
            let year = s.tech_year.unwrap_or(self.def.date.0);
            if s.units.is_empty() {
                out.push(format!("{}: NO UNITS", s.name));
            }
            for u in &s.units {
                match ds.vehicle(&u.vehicle) {
                    Some(v) if !v.in_service.contains(year) => {
                        out.push(format!("{} {} NOT IN SERVICE {year}", u.callsign, v.name.to_uppercase()))
                    }
                    None => out.push(format!("{}: UNKNOWN VEHICLE", u.callsign)),
                    _ => {}
                }
            }
        }
        out
    }

    pub fn draw_world(&self, f: &mut Frame, cam: &Camera) {
        for (k, o) in self.def.objectives.iter().enumerate() {
            let c = if self.sel == Some(Sel::Objective(k)) { style::SELECT } else { style::AMBER };
            let p = cam.to_screen(o.centre);
            f.circle_lines(p, (o.radius_m / cam.mpp) as f32, c);
            f.text(&o.name, p[0] - 16.0, p[1] - 6.0, style::FONT_L - 8, c);
        }
        for (si, side) in self.def.sides.iter().enumerate() {
            let c = if si == 0 { style::FRIENDLY } else { style::HOSTILE };
            for (ui, u) in side.units.iter().enumerate() {
                let p = cam.to_screen(u.position);
                if si == 0 {
                    symbols::friendly_armour(f, p, c);
                } else {
                    symbols::hostile_armour(f, p, c);
                }
                symbols::heading_staff(f, p, u.heading_deg.to_radians(), 14.0, c);
                f.text(&u.callsign, p[0] + 18.0, p[1] - 6.0, style::FONT_L - 10, c);
                if self.sel == Some(Sel::Unit(si, ui)) {
                    symbols::selection(f, p);
                }
            }
        }
    }

    pub fn draw_ui(&self, f: &mut Frame, screen: P2, ds: &DataSet) {
        let (x, y, w) = (10.0, 40.0, 300.0);
        let side = &self.def.sides[self.side];
        let vname = ds.vehicle(&self.vehicle).map_or("-".to_owned(), |v| v.name.to_uppercase());
        let mut lines = vec![
            (self.def.name.clone(), style::TEXT),
            (
                format!("DATE    {:04}-{:02}-{:02}   Y/U", self.def.date.0, self.def.date.1, self.def.date.2),
                style::TEXT,
            ),
            (format!("START   {}   T", format::clock(self.def.start_time_s as f64)), style::TEXT),
            (format!("SIDE    {} ({})   TAB", side.name, side.doctrine.to_uppercase()), style::TEXT),
            (format!("VEHICLE {vname}   V"), style::TEXT),
            (format!("PLATOON {}   N NEW", self.platoon[self.side].as_deref().unwrap_or("-")), style::TEXT),
            (
                format!(
                    "UNITS   {}   OBJECTIVES {}",
                    self.def.sides.iter().map(|s| s.units.len()).sum::<usize>(),
                    self.def.objectives.len()
                ),
                style::TEXT_DIM,
            ),
            (if self.unsaved { "UNSAVED  CTRL+S".to_owned() } else { "SAVED".to_owned() }, style::TEXT_DIM),
        ];
        for p in self.problems(ds).into_iter().take(4) {
            lines.push((p, style::HOSTILE));
        }
        let h = 36.0 + lines.len() as f32 * 18.0;
        f.rect(x, y, w, h, style::PANEL);
        f.rect_lines(x, y, w, h, style::PANEL_EDGE);
        f.text("SCENARIO EDITOR", x + 10.0, y + 8.0, style::FONT_L - 6, style::AMBER);
        for (k, (l, c)) in lines.iter().enumerate() {
            f.text(l, x + 10.0, y + 32.0 + k as f32 * 18.0, style::FONT_L - 8, *c);
        }
        f.text(
            "P PLACE  LMB SELECT/DRAG  R ROTATE  DEL REMOVE  O OBJECTIVE ([ ] RADIUS)  CTRL+S SAVE  F9 PLAY",
            10.0,
            screen[1] - 44.0,
            style::FONT_L - 8,
            style::TEXT_DIM,
        );
        f.text(&self.status, 330.0, 44.0, style::FONT_L - 8, style::AMBER);
    }

    pub fn over_ui(&self, s: P2) -> bool {
        s[0] < 310.0 && s[1] > 40.0 && s[1] < 300.0
    }
}
