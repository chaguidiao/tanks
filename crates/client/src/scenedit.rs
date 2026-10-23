//! Scenario editor (DESIGN.md §10.4, §11): place and orient units, group them into platoons, set
//! objectives, date and start time, weather, then save to `data/scenarios/<id>.ron` and play.
//!
//! The left panel has one page per area (UNITS, TIME / WEATHER). Each page is a list of value
//! rows: a click or `+` steps a value forward, `-` back; PGUP / PGDN move the keyboard focus over
//! the rows and `-` / `=` step the focused one. The panel is built by [`ScenarioEditor::buttons`]
//! (the same layout for hit-testing and drawing) and every hotkey issues the same [`EdAction`].

use sim_data::{DataSet, ObjectiveDef, ScenarioDef, SideDef, UnitDef};

use crate::format;
use crate::hud::{Button, Look, Rect};
use crate::render::{Frame, Input, Key, P2, Rgba};
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

// ---------------------------------------------------------------------------------------------
// Panel model: pages, value rows and the actions the buttons and hotkeys issue.
// ---------------------------------------------------------------------------------------------

/// A page of the editor panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Units,
    Weather,
}

impl Page {
    pub const ALL: [Page; 2] = [Page::Units, Page::Weather];

    fn label(self) -> &'static str {
        match self {
            Page::Units => "UNITS",
            Page::Weather => "TIME / WX",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Page::Units => "1",
            Page::Weather => "2",
        }
    }

    fn tip(self) -> &'static str {
        match self {
            Page::Units => "SIDES, DOCTRINE, VEHICLES AND PLATOONS",
            Page::Weather => "DATE, START TIME, WEATHER PRESET, WIND, LATITUDE AND MOON",
        }
    }

    fn fields(self) -> &'static [Field] {
        use Field as F;
        match self {
            Page::Units => &[F::Side, F::Doctrine, F::TechYear, F::Vehicle, F::Platoon],
            Page::Weather => {
                &[F::Year, F::Month, F::Day, F::Start, F::Preset, F::WindFrom, F::WindSpeed, F::Latitude, F::Moon]
            }
        }
    }
}

/// One editable value of the scenario.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Side,
    Doctrine,
    TechYear,
    Vehicle,
    Platoon,
    Year,
    Month,
    Day,
    Start,
    Preset,
    WindFrom,
    WindSpeed,
    Latitude,
    Moon,
}

/// What a panel button or an editor hotkey does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdAction {
    Page(Page),
    /// Step a value: +1 forward / up, -1 back / down.
    Step(Field, i8),
    /// Move the keyboard focus over the page's rows.
    Focus(i8),
}

/// Panel frame and row geometry (px).
const PANEL_X: f32 = 10.0;
const PANEL_Y: f32 = 40.0;
const PANEL_W: f32 = 340.0;
const ROW_H: f32 = 20.0;
const ROW_STEP: f32 = 22.0;
const LINE_H: f32 = 18.0;
/// Offsets from the panel top: page tabs, first value row.
const TABS_Y: f32 = 26.0;
const ROWS_Y: f32 = 56.0;
const STEP_W: f32 = 22.0;

/// Wind speed limit of the editor (m/s): a strong gale.
const WIND_MAX_MPS: f64 = 25.0;
/// Scenario year range the editor offers (the vehicle data spans the Cold War to today).
const YEARS: (i32, i32) = (1950, 2040);

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Step through a list: the index after (or before) `cur`, wrapping; 0 if `cur` is not in it.
fn cycle<T: PartialEq>(list: &[T], cur: &T, dir: i8) -> usize {
    let n = list.len().max(1) as i64;
    list.iter().position(|v| v == cur).map_or(0, |k| (k as i64 + i64::from(dir)).rem_euclid(n) as usize)
}

/// Compass point of a bearing (deg), for the wind row.
fn compass(deg: f64) -> &'static str {
    const P: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
    P[((deg.rem_euclid(360.0) + 22.5) / 45.0) as usize % 8]
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
    page: Page,
    /// Keyboard focus: index into the page's rows.
    focus: usize,
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
                fire_support: vec![],
                supply_points: vec![],
            });
        }
        let vehicle =
            def.sides[0].units.first().map(|u| u.vehicle.clone()).unwrap_or_else(|| {
                ds.vehicles_in_service(def.date.0).next().map_or_else(String::new, |v| v.id.clone())
            });
        let platoon = def.sides.iter().map(|s| s.units.last().and_then(|u| u.platoon.clone())).collect();
        Self {
            def,
            side: 0,
            vehicle,
            platoon,
            sel: None,
            dragging: false,
            page: Page::Units,
            focus: 0,
            unsaved: false,
            status: String::new(),
        }
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

    /// Editor hotkeys, as the same actions the panel buttons issue.
    fn hotkeys(&self, inp: &Input) -> Vec<EdAction> {
        let mut a = Vec::new();
        for (k, p) in Page::ALL.iter().enumerate() {
            if inp.hit(Key::Num(k as u8 + 1)) {
                a.push(EdAction::Page(*p));
            }
        }
        if inp.hit(Key::PageUp) {
            a.push(EdAction::Focus(-1));
        }
        if inp.hit(Key::PageDown) {
            a.push(EdAction::Focus(1));
        }
        if let Some(&f) = self.page.fields().get(self.focus) {
            if inp.hit(Key::Minus) {
                a.push(EdAction::Step(f, -1));
            }
            if inp.hit(Key::Equal) {
                a.push(EdAction::Step(f, 1));
            }
        }
        let keys = [
            (Key::Tab, Field::Side, 1),
            (Key::Char('v'), Field::Vehicle, 1),
            (Key::Char('n'), Field::Platoon, 1),
            (Key::Char('y'), Field::Year, -1),
            (Key::Char('u'), Field::Year, 1),
            (Key::Char('t'), Field::Start, 1),
        ];
        for (k, f, d) in keys {
            if inp.hit(k) {
                a.push(EdAction::Step(f, d));
            }
        }
        a
    }

    pub fn update(
        &mut self,
        inp: &Input,
        cam: &Camera,
        ds: &DataSet,
        over_ui: bool,
        click: Option<EdAction>,
    ) -> Effects {
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
        if inp.hit(Key::Char('o')) && !over_ui {
            let name = format!("OBJ {}", self.def.objectives.len() + 1);
            self.def.objectives.push(ObjectiveDef { name: name.clone(), centre: m, radius_m: 300.0 });
            self.sel = Some(Sel::Objective(self.def.objectives.len() - 1));
            self.status = format!("OBJECTIVE {name}");
            self.unsaved = true;
        }
        if inp.hit(Key::Delete) || inp.hit(Key::Backspace) {
            self.delete_selected();
        }
        if let Some(Sel::Objective(k)) = self.sel {
            let o = &mut self.def.objectives[k];
            if inp.hit(Key::LeftBracket) {
                o.radius_m = (o.radius_m - 50.0).max(100.0);
                self.unsaved = true;
            }
            if inp.hit(Key::RightBracket) {
                o.radius_m = (o.radius_m + 50.0).min(1500.0);
                self.unsaved = true;
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
        let mut actions = self.hotkeys(inp);
        actions.extend(click);
        for a in actions {
            self.act(a, ds);
        }
        if inp.ctrl && inp.hit(Key::Char('s')) {
            self.prepare_save();
            fx.save = true;
        }
        if inp.hit(Key::F(9)) {
            fx.play = true;
        }
        fx
    }

    fn delete_selected(&mut self) {
        match self.sel.take() {
            Some(Sel::Unit(s, u)) => {
                let gone = self.def.sides[s].units.remove(u);
                self.status = format!("REMOVED {}", gone.callsign);
            }
            Some(Sel::Objective(k)) => {
                let gone = self.def.objectives.remove(k);
                self.status = format!("REMOVED {}", gone.name);
            }
            None => return,
        }
        self.unsaved = true;
    }

    /// Run one panel action (button or hotkey).
    pub fn act(&mut self, a: EdAction, ds: &DataSet) {
        match a {
            EdAction::Page(p) => {
                self.page = p;
                self.focus = 0;
            }
            EdAction::Focus(d) => {
                let n = self.page.fields().len().max(1) as i64;
                self.focus = (self.focus as i64 + i64::from(d)).rem_euclid(n) as usize;
            }
            EdAction::Step(f, d) => {
                if let Some(k) = self.page.fields().iter().position(|x| *x == f) {
                    self.focus = k;
                }
                self.step(f, d, ds);
            }
        }
    }

    /// Change one value by one step in direction `dir` (+1 / -1).
    fn step(&mut self, f: Field, dir: i8, ds: &DataSet) {
        let d = f64::from(dir);
        let s = self.side;
        match f {
            Field::Side => {
                let n = self.def.sides.len() as i64;
                self.side = (s as i64 + i64::from(dir)).rem_euclid(n) as usize;
                self.sel = None;
                self.status = format!("SIDE {}", self.def.sides[self.side].name);
                return;
            }
            Field::Vehicle => {
                let list: Vec<&str> = ds.vehicles_in_service(self.year()).map(|v| v.id.as_str()).collect();
                if list.is_empty() {
                    return;
                }
                self.vehicle = list[cycle(&list, &self.vehicle.as_str(), dir)].to_owned();
                let Some(Sel::Unit(s, u)) = self.sel else { return };
                self.def.sides[s].units[u].vehicle = self.vehicle.clone();
            }
            Field::Platoon => {
                self.platoon[s] = Some(self.new_platoon());
                self.status = format!("NEW PLATOON {}", self.platoon[s].as_deref().unwrap_or(""));
                return;
            }
            Field::Doctrine => {
                let list: Vec<&str> = ds.doctrine.iter().map(|p| p.id.as_str()).collect();
                if list.is_empty() {
                    return;
                }
                let cur = self.def.sides[s].doctrine.clone();
                self.def.sides[s].doctrine = list[cycle(&list, &cur.as_str(), dir)].to_owned();
            }
            Field::TechYear => {
                let date = self.def.date.0;
                let t = (self.year() + i32::from(dir)).clamp(YEARS.0, YEARS.1);
                self.def.sides[s].tech_year = (t != date).then_some(t);
            }
            Field::Year => {
                let y = &mut self.def.date.0;
                *y = (*y + i32::from(dir)).clamp(YEARS.0, YEARS.1);
                self.clamp_day();
            }
            Field::Month => {
                let m = i32::from(self.def.date.1) - 1 + i32::from(dir);
                self.def.date.1 = m.rem_euclid(12) as u8 + 1;
                self.clamp_day();
            }
            Field::Day => {
                let n = i32::from(days_in_month(self.def.date.0, self.def.date.1));
                let day = i32::from(self.def.date.2) - 1 + i32::from(dir);
                self.def.date.2 = day.rem_euclid(n) as u8 + 1;
            }
            Field::Start => {
                let t = i64::from(self.def.start_time_s) + i64::from(dir) * 1800;
                self.def.start_time_s = t.rem_euclid(86_400) as u32;
            }
            Field::Preset => {
                let list: Vec<&str> = ds.weather.presets.iter().map(|p| p.id.as_str()).collect();
                if list.is_empty() {
                    return;
                }
                let cur = self.def.weather.preset.clone();
                self.def.weather.preset = list[cycle(&list, &cur.as_str(), dir)].to_owned();
            }
            Field::WindFrom => {
                let w = &mut self.def.weather.wind_from_deg;
                *w = (*w + 15.0 * d).rem_euclid(360.0);
            }
            Field::WindSpeed => {
                let w = &mut self.def.weather.wind_speed_mps;
                *w = (*w + d).clamp(0.0, WIND_MAX_MPS);
            }
            Field::Latitude => {
                let l = &mut self.def.weather.latitude_deg;
                *l = (*l + 5.0 * d).clamp(-70.0, 70.0);
            }
            Field::Moon => {
                let m = &mut self.def.weather.moon_phase;
                *m = (*m + 0.25 * d).clamp(0.0, 1.0);
            }
        }
        self.unsaved = true;
    }

    fn clamp_day(&mut self) {
        let (y, m, _) = self.def.date;
        self.def.date.2 = self.def.date.2.clamp(1, days_in_month(y, m));
    }

    fn field_label(f: Field) -> &'static str {
        match f {
            Field::Side => "SIDE",
            Field::Doctrine => "DOCTRINE",
            Field::TechYear => "TECH YEAR",
            Field::Vehicle => "VEHICLE",
            Field::Platoon => "PLATOON",
            Field::Year => "YEAR",
            Field::Month => "MONTH",
            Field::Day => "DAY",
            Field::Start => "START TIME",
            Field::Preset => "WEATHER",
            Field::WindFrom => "WIND FROM",
            Field::WindSpeed => "WIND SPEED",
            Field::Latitude => "LATITUDE",
            Field::Moon => "MOON",
        }
    }

    fn field_tip(f: Field) -> &'static str {
        match f {
            Field::Side => "SIDE THAT P PLACES UNITS FOR",
            Field::Doctrine => "DOCTRINE PACK OF THE SIDE (DATA/DOCTRINE)",
            Field::TechYear => "PRETEND THE SIDE LIVES IN THIS YEAR (OLDER OR NEWER EQUIPMENT)",
            Field::Vehicle => "VEHICLE TYPE P PLACES (IN SERVICE AT THE SIDE'S YEAR); ALSO SETS THE SELECTED UNIT",
            Field::Platoon => "START A NEW PLATOON FOR THE NEXT PLACED UNIT",
            Field::Year => "SCENARIO YEAR: DECIDES WHICH VEHICLES EXIST",
            Field::Month => "SCENARIO MONTH: SUN AND DAY LENGTH",
            Field::Day => "SCENARIO DAY OF THE MONTH",
            Field::Start => "LOCAL SOLAR TIME AT THE START, 30 MIN STEPS",
            Field::Preset => {
                "WEATHER PRESET: VISIBILITY, THERMAL EXTINCTION, SKY LIGHT, GOING (DATA/COMBAT/WEATHER.RON)"
            }
            Field::WindFrom => "DIRECTION THE WIND BLOWS FROM: DRIFTS SMOKE, 15° STEPS",
            Field::WindSpeed => "WIND SPEED: HOW FAST SMOKE DRIFTS",
            Field::Latitude => "LATITUDE OF THE MAP: SUN ELEVATION, DAWN AND DUSK",
            Field::Moon => "MOON ILLUMINATION AT NIGHT (0 NEW, 1 FULL)",
        }
    }

    fn field_value(&self, f: Field, ds: &DataSet) -> String {
        let side = &self.def.sides[self.side];
        let w = &self.def.weather;
        match f {
            Field::Side => side.name.clone(),
            Field::Doctrine => side.doctrine.to_uppercase(),
            Field::TechYear => side.tech_year.map_or("AS DATE".into(), |y| y.to_string()),
            Field::Vehicle => ds.vehicle(&self.vehicle).map_or("-".to_owned(), |v| v.name.to_uppercase()),
            Field::Platoon => self.platoon[self.side].as_deref().unwrap_or("-").to_owned(),
            Field::Year => self.def.date.0.to_string(),
            Field::Month => format!("{:02}", self.def.date.1),
            Field::Day => format!("{:02}", self.def.date.2),
            Field::Start => format::clock(f64::from(self.def.start_time_s)),
            Field::Preset => ds.weather.preset(&w.preset).map_or(w.preset.to_uppercase(), |p| p.name.to_uppercase()),
            Field::WindFrom => format!("{} {:03.0}°", compass(w.wind_from_deg), w.wind_from_deg),
            Field::WindSpeed => format!("{:.0} M/S", w.wind_speed_mps),
            Field::Latitude => {
                format!("{:.0}° {}", w.latitude_deg.abs(), if w.latitude_deg < 0.0 { "S" } else { "N" })
            }
            Field::Moon => format!("{:.2}", w.moon_phase),
        }
    }

    /// Panel buttons: page tabs, then the page's value rows with their - / + steps.
    pub fn buttons(&self, ds: &DataSet) -> Vec<Button<EdAction>> {
        let mut out = Vec::new();
        let (x, y, w) = (PANEL_X, PANEL_Y, PANEL_W);
        let n = Page::ALL.len() as f32;
        let tw = (w - 12.0 - (n - 1.0) * 4.0) / n;
        for (k, p) in Page::ALL.iter().enumerate() {
            let r = Rect::new(x + 6.0 + k as f32 * (tw + 4.0), y + TABS_Y, tw, ROW_H + 2.0);
            out.push(
                Button::new(r, Look::Bar, format!("{} {}", p.key(), p.label()), EdAction::Page(*p))
                    .key(p.key())
                    .tip(p.tip())
                    .active(self.page == *p),
            );
        }
        for (k, f) in self.page.fields().iter().enumerate() {
            let ry = y + ROWS_Y + k as f32 * ROW_STEP;
            let rw = w - 12.0 - 2.0 * (STEP_W + 2.0);
            let focus = self.focus == k;
            out.push(
                Button::new(Rect::new(x + 6.0, ry, rw, ROW_H), Look::Row, Self::field_label(*f), EdAction::Step(*f, 1))
                    .value(self.field_value(*f, ds))
                    .tip(Self::field_tip(*f))
                    .active(focus),
            );
            let sx = x + 6.0 + rw + 2.0;
            out.push(
                Button::new(Rect::new(sx, ry, STEP_W, ROW_H), Look::Bar, "-", EdAction::Step(*f, -1))
                    .key("-")
                    .tip("PREVIOUS VALUE (PGUP / PGDN FOCUS A ROW)"),
            );
            out.push(
                Button::new(Rect::new(sx + STEP_W + 2.0, ry, STEP_W, ROW_H), Look::Bar, "+", EdAction::Step(*f, 1))
                    .key("=")
                    .tip("NEXT VALUE (PGUP / PGDN FOCUS A ROW)"),
            );
        }
        out
    }

    /// Info lines below the rows: page notes, totals, save state and problems.
    fn info_lines(&self, ds: &DataSet) -> Vec<(String, Rgba)> {
        let mut lines = Vec::new();
        match self.page {
            Page::Units => {
                lines.push((
                    format!(
                        "UNITS {}   OBJECTIVES {}",
                        self.def.sides.iter().map(|s| s.units.len()).sum::<usize>(),
                        self.def.objectives.len()
                    ),
                    style::TEXT_DIM,
                ));
            }
            Page::Weather => lines.extend(self.weather_lines(ds)),
        }
        lines.push((if self.unsaved { "UNSAVED  CTRL+S".to_owned() } else { "SAVED".to_owned() }, style::TEXT_DIM));
        for p in self.problems(ds).into_iter().take(4) {
            lines.push((p, style::HOSTILE));
        }
        lines
    }

    /// What the weather page's settings give at the start time (sim formulas, display only).
    fn weather_lines(&self, ds: &DataSet) -> Vec<(String, Rgba)> {
        let w = &self.def.weather;
        let Some(p) = ds.weather.preset(&w.preset) else {
            return vec![(format!("UNKNOWN WEATHER {}", w.preset.to_uppercase()), style::HOSTILE)];
        };
        let hour = f64::from(self.def.start_time_s) / 3600.0;
        let sun = sim::weather::sun_elevation_deg(w.latitude_deg, self.def.date, 0, hour);
        let light = sim::weather::light_factor(sun, w.moon_phase, p.sky_light.get(), &ds.detection);
        let mut v = vec![
            (format!("VISIBILITY {}", format::distance(p.visibility_m.get())), style::TEXT_DIM),
            (format!("SUN {sun:+.0}° AT START   LIGHT {:.0} %", light * 100.0), style::TEXT_DIM),
        ];
        if p.traction_factor.get() < 0.9 {
            v.push(("GOING SOFT".into(), style::TEXT_DIM));
        }
        v
    }

    /// Panel frame (also the area that swallows map clicks).
    fn panel_rect(&self, ds: &DataSet) -> Rect {
        let rows = self.page.fields().len() as f32;
        let lines = self.info_lines(ds).len() as f32;
        Rect::new(PANEL_X, PANEL_Y, PANEL_W, ROWS_Y + rows * ROW_STEP + 6.0 + lines * LINE_H + 6.0)
    }

    /// Saving never overwrites a shipped scenario: edits go to `<id>-custom`.
    pub fn prepare_save(&mut self) {
        if !self.def.id.ends_with("-custom") {
            self.def.id.push_str("-custom");
            self.def.name.push_str(" (CUSTOM)");
        }
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

    /// Panel frame, title and info lines; the buttons are drawn by the map screen.
    pub fn draw_ui(&self, f: &mut Frame, screen: P2, ds: &DataSet) {
        let r = self.panel_rect(ds);
        f.rect(r.x, r.y, r.w, r.h, style::PANEL);
        f.rect_lines(r.x, r.y, r.w, r.h, style::PANEL_EDGE);
        f.text("SCENARIO", r.x + 10.0, r.y + 6.0, style::FONT_S, style::AMBER);
        let name = self.def.name.to_uppercase();
        let nw = f.text_width(&name, style::FONT_S);
        let nx = (r.x + r.w - 10.0 - nw).max(r.x + 90.0);
        f.text(&name, nx, r.y + 6.0, style::FONT_S, style::TEXT);
        let y0 = r.y + ROWS_Y + self.page.fields().len() as f32 * ROW_STEP + 6.0;
        for (k, (l, c)) in self.info_lines(ds).iter().enumerate() {
            f.text(l, r.x + 10.0, y0 + k as f32 * LINE_H, style::FONT_S, *c);
        }
        let help = match self.page {
            Page::Units => "P PLACE  LMB SELECT/DRAG  R ROTATE  DEL REMOVE  O OBJECTIVE ([ ] RADIUS)",
            Page::Weather => "CLICK OR + NEXT VALUE  - PREVIOUS  PGUP/PGDN FOCUS, - / = STEP",
        };
        f.text(
            &format!("{help}   1-{} PAGES  CTRL+S SAVE  F9 PLAY TEST  ESC MENU", Page::ALL.len()),
            10.0,
            screen[1] - 44.0,
            style::FONT_S,
            style::TEXT_DIM,
        );
        f.text(&self.status, r.x + r.w + 12.0, 44.0, style::FONT_S, style::AMBER);
    }

    pub fn over_ui(&self, s: P2, ds: &DataSet) -> bool {
        self.panel_rect(ds).contains(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn data_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")
    }

    /// Every field, exactly (`f64` debug output round-trips).
    fn dump(def: &ScenarioDef) -> String {
        format!("{def:?}")
    }

    #[test]
    fn steps_wrap_and_clamp() {
        let ds = DataSet::load(&data_dir()).unwrap();
        let def = DataSet::load_scenario(&data_dir(), "m1-move").unwrap();
        let mut ed = ScenarioEditor::new(def, &ds);
        ed.def.date = (2000, 1, 31);
        ed.act(EdAction::Step(Field::Month, 1), &ds);
        assert_eq!(ed.def.date, (2000, 2, 29), "leap February clamps the day");
        ed.act(EdAction::Step(Field::Month, -1), &ds);
        ed.act(EdAction::Step(Field::Month, -1), &ds);
        assert_eq!(ed.def.date.1, 12, "months wrap");
        ed.def.start_time_s = 600;
        ed.act(EdAction::Step(Field::Start, -1), &ds);
        assert_eq!(ed.def.start_time_s, 86_400 - 1200);
        ed.act(EdAction::Step(Field::WindFrom, -1), &ds);
        assert_eq!(ed.def.weather.wind_from_deg, 345.0);
        ed.act(EdAction::Step(Field::WindSpeed, -1), &ds);
        assert_eq!(ed.def.weather.wind_speed_mps, 0.0);
        for _ in 0..10 {
            ed.act(EdAction::Step(Field::Moon, 1), &ds);
        }
        assert_eq!(ed.def.weather.moon_phase, 1.0);
        let presets = ds.weather.presets.len();
        for _ in 0..presets {
            ed.act(EdAction::Step(Field::Preset, 1), &ds);
        }
        assert_eq!(ed.def.weather.preset, "clear", "presets cycle back");
        ed.act(EdAction::Step(Field::TechYear, 1), &ds);
        assert_eq!(ed.def.sides[0].tech_year, Some(ed.def.date.0 + 1));
        ed.act(EdAction::Step(Field::TechYear, -1), &ds);
        assert_eq!(ed.def.sides[0].tech_year, None, "back at the scenario date means no override");
    }

    /// A scenario edited and saved by the editor loads back identically, and every shipped
    /// scenario survives an unedited save.
    #[test]
    fn saved_scenarios_load_back_identically() {
        let ds = DataSet::load(&data_dir()).unwrap();
        let tmp = std::env::temp_dir().join(format!("tanks-scenedit-{}", std::process::id()));
        std::fs::create_dir_all(tmp.join("scenarios")).unwrap();
        for id in DataSet::scenario_ids(&data_dir()) {
            let def = DataSet::load_scenario(&data_dir(), &id).unwrap();
            let mut ed = ScenarioEditor::new(def, &ds);
            ed.prepare_save();
            DataSet::save_scenario(&tmp, &ed.def).unwrap();
            let back = DataSet::load_scenario(&tmp, &ed.def.id).unwrap();
            assert_eq!(dump(&back), dump(&ed.def), "{id}");
        }
        let def = DataSet::load_scenario(&data_dir(), "m1-move").unwrap();
        let mut ed = ScenarioEditor::new(def, &ds);
        for (f, d) in [
            (Field::Preset, 1),
            (Field::WindFrom, 1),
            (Field::WindSpeed, 1),
            (Field::Latitude, -1),
            (Field::Moon, 1),
            (Field::Month, 1),
            (Field::Day, 1),
            (Field::Start, 1),
            (Field::TechYear, 1),
            (Field::Doctrine, 1),
        ] {
            ed.act(EdAction::Step(f, d), &ds);
        }
        ed.prepare_save();
        DataSet::save_scenario(&tmp, &ed.def).unwrap();
        let back = DataSet::load_scenario(&tmp, &ed.def.id).unwrap();
        assert_eq!(dump(&back), dump(&ed.def));
        assert_ne!(back.weather, sim_data::WeatherDef::default());
        std::fs::remove_dir_all(&tmp).ok();
    }
}
