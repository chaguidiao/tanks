//! Scenario editor (DESIGN.md §10.4, §11): place and orient units, group them into platoons, set
//! objectives, supply points (M8c), date and start time, weather, then save to `data/scenarios/<id>.ron` and play.
//!
//! The left panel has one page per area (UNITS, TIME / WEATHER). Each page is a list of value
//! rows: a click or `+` steps a value forward, `-` back; PGUP / PGDN move the keyboard focus over
//! the rows and `-` / `=` step the focused one. The panel is built by [`ScenarioEditor::buttons`]
//! (the same layout for hit-testing and drawing) and every hotkey issues the same [`EdAction`].

use sim_data::{BatteryAssign, DataSet, ObjectiveDef, ScenarioDef, SideDef, SupplyPointDef, UnitDef, VictoryDef};

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
    /// A fire-support battery: (side, index in `fire_support`).
    Battery(usize, usize),
    /// Supply point (side, index).
    Supply(usize, usize),
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
    Fires,
    Victory,
}

impl Page {
    pub const ALL: [Page; 4] = [Page::Units, Page::Weather, Page::Fires, Page::Victory];

    fn label(self) -> &'static str {
        match self {
            Page::Units => "UNITS",
            Page::Weather => "TIME / WX",
            Page::Fires => "FIRES",
            Page::Victory => "VICTORY",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Page::Units => "1",
            Page::Weather => "2",
            Page::Fires => "3",
            Page::Victory => "4",
        }
    }

    fn tip(self) -> &'static str {
        match self {
            Page::Units => "SIDES, DOCTRINE, VEHICLES AND PLATOONS",
            Page::Weather => "DATE, START TIME, WEATHER PRESET, WIND, LATITUDE AND MOON",
            Page::Fires => "FIRE SUPPORT: BATTERIES PER SIDE (B PLACES ONE AT THE CURSOR)",
            Page::Victory => "VICTORY CONDITIONS: TIME LIMIT, OBJECTIVE, ATTACKER, LOSS LIMIT",
        }
    }

    fn fields(self) -> &'static [Field] {
        use Field as F;
        match self {
            Page::Units => &[F::Side, F::Doctrine, F::TechYear, F::Vehicle, F::Platoon],
            Page::Fires => &[F::Side, F::Battery, F::BatteryType, F::BatteryHe, F::BatterySmoke],
            Page::Victory => &[F::VicOn, F::VicTime, F::VicObjective, F::VicAttacker, F::VicLoss],
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
    Battery,
    BatteryType,
    BatteryHe,
    BatterySmoke,
    VicOn,
    VicTime,
    VicObjective,
    VicAttacker,
    VicLoss,
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
    /// Serving radius of a supply point (`logistics.ron`), for drawing.
    point_radius_m: f64,
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
            point_radius_m: ds.logistics.point_radius_m.get(),
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
        for (si, side) in self.def.sides.iter().enumerate() {
            for (k, b) in side.fire_support.iter().enumerate() {
                let dist = d(b.position);
                if dist < PICK_PX && best.is_none_or(|(b, _)| dist < b) {
                    best = Some((dist, Sel::Battery(si, k)));
                }
            }
            for (k, sp) in side.supply_points.iter().enumerate() {
                // The symbol's pin stands above its point.
                let dist = d(sp.position).min(d([sp.position[0], sp.position[1] + 16.0 * cam.mpp]));
                if dist < PICK_PX && best.is_none_or(|(b, _)| dist < b) {
                    best = Some((dist, Sel::Supply(si, k)));
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

    /// A new supply point for the current side, stocked for one refill of a four-tank platoon of
    /// each vehicle type the side fields (a starting point to edit: [ ] halve / double it).
    fn place_supply(&mut self, at: [f64; 2], ds: &DataSet) {
        let side = self.side;
        let year = self.year();
        let mut rounds: Vec<(String, u32)> = Vec::new();
        let (mut fuel, mut smoke) = (0.0f64, 0u32);
        let mut types: Vec<&str> = self.def.sides[side].units.iter().map(|u| u.vehicle.as_str()).collect();
        types.sort_unstable();
        types.dedup();
        for v in types.iter().filter_map(|id| ds.vehicle(id)) {
            fuel = fuel.max(v.mobility.fuel_capacity_l.get() * PLATOON_SIZE as f64);
            smoke = smoke.max(v.smoke_launcher.as_ref().map_or(0, |l| l.salvos) * PLATOON_SIZE as u32);
            for l in &v.armament.load {
                if !ds.ammo(&l.ammo).is_some_and(|a| a.in_service.contains(year)) {
                    continue;
                }
                let n = l.count * PLATOON_SIZE as u32;
                match rounds.iter_mut().find(|(id, _)| *id == l.ammo) {
                    Some(r) => r.1 = r.1.max(n),
                    None => rounds.push((l.ammo.clone(), n)),
                }
            }
        }
        let n = self.def.sides.iter().map(|s| s.supply_points.len()).sum::<usize>() + 1;
        let callsign = format!("SP {n}");
        self.def.sides[side].supply_points.push(SupplyPointDef {
            callsign: callsign.clone(),
            position: at,
            unlimited: false,
            fuel_l: fuel.round(),
            rounds,
            smoke_salvos: smoke,
        });
        self.sel = Some(Sel::Supply(side, self.def.sides[side].supply_points.len() - 1));
        self.status = format!("SUPPLY POINT {callsign}");
        self.unsaved = true;
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
                Some(Sel::Battery(s, k)) => self.def.sides[s].fire_support[k].position = m,
                Some(Sel::Supply(s, k)) => self.def.sides[s].supply_points[k].position = m,
                None => {}
            }
            self.unsaved = true;
        }
        if inp.hit(Key::Char('s')) && !inp.ctrl && !over_ui {
            self.place_supply(m, ds);
        }

        if inp.hit(Key::Char('p')) && !over_ui {
            self.place(m);
        }
        if inp.hit(Key::Char('b')) && !over_ui {
            self.place_battery(m, ds);
        }
        if inp.hit(Key::Char('o')) && !over_ui {
            let taken = |n: &str| self.def.objectives.iter().any(|o| o.name == n);
            let name = (1..).map(|n| format!("OBJ {n}")).find(|n| !taken(n)).expect("a free name");
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
        if let Some(Sel::Supply(s, k)) = self.sel {
            let sp = &mut self.def.sides[s].supply_points[k];
            let scale = match (inp.hit(Key::LeftBracket), inp.hit(Key::RightBracket)) {
                (true, _) => Some(0.5),
                (_, true) => Some(2.0),
                _ => None,
            };
            if let Some(f) = scale {
                sp.fuel_l = (sp.fuel_l * f).round();
                sp.smoke_salvos = (sp.smoke_salvos as f64 * f) as u32;
                for r in &mut sp.rounds {
                    r.1 = (r.1 as f64 * f) as u32;
                }
                self.unsaved = true;
            }
            if inp.hit(Key::Char('r')) && !inp.ctrl {
                sp.unlimited = !sp.unlimited;
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
            Some(Sel::Battery(s, k)) => {
                let gone = self.def.sides[s].fire_support.remove(k);
                self.status = format!("REMOVED {}", gone.callsign);
            }
            Some(Sel::Supply(s, k)) => {
                let gone = self.def.sides[s].supply_points.remove(k);
                self.status = format!("REMOVED {}", gone.callsign);
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
            Field::Battery | Field::BatteryType | Field::BatteryHe | Field::BatterySmoke => {
                if !self.step_fires(f, dir, ds) {
                    return;
                }
            }
            Field::VicOn | Field::VicTime | Field::VicObjective | Field::VicAttacker | Field::VicLoss => {
                if !self.step_victory(f, dir) {
                    return;
                }
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
            Field::Battery => "BATTERY",
            Field::BatteryType => "TYPE",
            Field::BatteryHe => "HE ROUNDS",
            Field::BatterySmoke => "SMOKE ROUNDS",
            Field::VicOn => "VICTORY",
            Field::VicTime => "TIME LIMIT",
            Field::VicObjective => "OBJECTIVE",
            Field::VicAttacker => "ATTACKER",
            Field::VicLoss => "LOSS LIMIT",
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
            Field::Battery => "SELECT THE NEXT BATTERY OF THE SIDE (B PLACES A NEW ONE AT THE CURSOR, DEL REMOVES)",
            Field::BatteryType => "BATTERY TYPE (DATA/COMBAT/FIRES.RON), IN SERVICE AT THE SIDE'S YEAR",
            Field::BatteryHe => "HE ROUNDS IN STOCK; TYPE = THE TYPE'S OWN STOCK",
            Field::BatterySmoke => "SMOKE ROUNDS IN STOCK; TYPE = THE TYPE'S OWN STOCK",
            Field::VicOn => "VICTORY CONDITIONS ON OR OFF (OFF: THE SCENARIO RUNS UNTIL STOPPED)",
            Field::VicTime => {
                "SCENARIO LENGTH, 5 MIN STEPS: AT THE END THE OBJECTIVE HOLDER WINS, TIES TO THE DEFENDER"
            }
            Field::VicObjective => "OBJECTIVE THE ATTACKER MUST SEIZE (HOLD ALONE FOR 2 MIN)",
            Field::VicAttacker => "ATTACKING SIDE; THE OTHER SIDE DEFENDS",
            Field::VicLoss => "A SIDE THAT LOSES THIS SHARE OF ITS VEHICLES LOSES AT ONCE",
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
            Field::Battery | Field::BatteryType | Field::BatteryHe | Field::BatterySmoke => self.fires_value(f, ds),
            Field::VicOn | Field::VicTime | Field::VicObjective | Field::VicAttacker | Field::VicLoss => {
                self.victory_value(f)
            }
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
            Page::Fires => lines.extend(self.fires_lines(ds)),
            Page::Victory => lines.extend(self.victory_lines()),
        }
        lines.extend(self.supply_lines(ds));
        lines.push((if self.unsaved { "UNSAVED  CTRL+S".to_owned() } else { "SAVED".to_owned() }, style::TEXT_DIM));
        for p in self.problems(ds).into_iter().take(4) {
            lines.push((p, style::HOSTILE));
        }
        lines
    }

    /// The selected supply point's stock (M8c), on every page.
    fn supply_lines(&self, ds: &DataSet) -> Vec<(String, Rgba)> {
        let Some(Sel::Supply(s, k)) = self.sel else { return Vec::new() };
        let sp = &self.def.sides[s].supply_points[k];
        let mut lines = vec![(
            format!("{} ({})   [ ] HALF/DOUBLE  R UNLIMITED", sp.callsign, self.def.sides[s].name),
            style::AMBER,
        )];
        if sp.unlimited {
            lines.push(("STOCK   UNLIMITED".into(), style::TEXT));
        } else {
            lines.push((format!("STOCK   {:.0} L   SMOKE SALVOS {}", sp.fuel_l, sp.smoke_salvos), style::TEXT));
            let rounds: Vec<String> = sp
                .rounds
                .iter()
                .map(|(id, n)| format!("{} {n}", ds.ammo(id).map_or(id.to_uppercase(), |a| a.kind.label().to_owned())))
                .collect();
            lines.push((
                format!("        {}", if rounds.is_empty() { "NO ROUNDS".into() } else { rounds.join(" · ") }),
                style::TEXT,
            ));
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
            for b in &s.fire_support {
                match ds.fires.battery(&b.battery) {
                    Some(d) if !d.in_service.contains(year) => {
                        out.push(format!("{} {} NOT IN SERVICE {year}", b.callsign, d.name.to_uppercase()))
                    }
                    None => out.push(format!("{}: UNKNOWN BATTERY", b.callsign)),
                    _ => {}
                }
            }
        }
        if let Some(v) = &self.def.victory {
            if let Some(o) = &v.objective
                && !self.def.objectives.iter().any(|x| &x.name == o)
            {
                out.push(format!("VICTORY OBJECTIVE {o} MISSING"));
            }
            if v.attacker >= self.def.sides.len() {
                out.push("VICTORY ATTACKER: NO SUCH SIDE".into());
            }
        }
        out
    }

    pub fn draw_world(&self, f: &mut Frame, cam: &Camera, ds: &DataSet) {
        self.draw_batteries(f, cam, ds);
        for (k, o) in self.def.objectives.iter().enumerate() {
            let c = if self.sel == Some(Sel::Objective(k)) { style::SELECT } else { style::AMBER };
            let p = cam.to_screen(o.centre);
            f.circle_lines(p, (o.radius_m / cam.mpp) as f32, c);
            if self.page == Page::Victory && self.victory_objective() == Some(o.name.as_str()) {
                f.circle_lines(p, (o.radius_m / cam.mpp) as f32 + 4.0, c);
            }
            f.text(&o.name, p[0] - 16.0, p[1] - 6.0, style::FONT_L - 8, c);
        }
        for (si, side) in self.def.sides.iter().enumerate() {
            let c = if si == 0 { style::FRIENDLY } else { style::HOSTILE };
            for (k, sp) in side.supply_points.iter().enumerate() {
                let c = if self.sel == Some(Sel::Supply(si, k)) { style::SELECT } else { c };
                symbols::supply_point(f, cam, sp.position, self.point_radius_m, &sp.callsign, c);
            }
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
            Page::Units => "P PLACE  LMB SELECT/DRAG  R ROTATE  DEL REMOVE  O OBJECTIVE ([ ] RADIUS)  S SUPPLY POINT",
            Page::Weather => "CLICK OR + NEXT VALUE  - PREVIOUS  PGUP/PGDN FOCUS, - / = STEP",
            Page::Fires => "B PLACE BATTERY AT CURSOR  LMB SELECT/DRAG  DEL REMOVE",
            Page::Victory => "O OBJECTIVE AT CURSOR ([ ] RADIUS)  CLICK OR + NEXT VALUE  - PREVIOUS",
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

// ---------------------------------------------------------------------------------------------
// Fire support (M7d): batteries per side, `SideDef.fire_support`.
// ---------------------------------------------------------------------------------------------

/// Rounds step of the HE and smoke stock rows.
const ROUNDS_STEP: u32 = 10;

impl ScenarioEditor {
    /// The selected battery, if it belongs to the side being edited.
    fn battery_sel(&self) -> Option<usize> {
        match self.sel {
            Some(Sel::Battery(s, k)) if s == self.side => Some(k),
            _ => None,
        }
    }

    /// Place a battery of the side at `at`: the type of the selected one, else the first type in
    /// service at the side's year.
    fn place_battery(&mut self, at: [f64; 2], ds: &DataSet) {
        let year = self.year();
        let kind = self
            .battery_sel()
            .map(|k| self.def.sides[self.side].fire_support[k].battery.clone())
            .or_else(|| ds.fires.batteries.iter().find(|b| b.in_service.contains(year)).map(|b| b.id.clone()))
            .or_else(|| ds.fires.batteries.first().map(|b| b.id.clone()));
        let Some(battery) = kind else {
            self.status = "NO BATTERY TYPES IN DATA".into();
            return;
        };
        let taken = |c: &str| self.def.sides.iter().flat_map(|s| &s.fire_support).any(|b| b.callsign == c);
        let callsign = (1..).map(|n| format!("ARTY {n}")).find(|c| !taken(c)).expect("a free callsign");
        let side = &mut self.def.sides[self.side];
        side.fire_support.push(BatteryAssign {
            callsign: callsign.clone(),
            battery,
            position: at,
            he_rounds: None,
            smoke_rounds: None,
        });
        self.sel = Some(Sel::Battery(self.side, side.fire_support.len() - 1));
        self.page = Page::Fires;
        self.focus = 1;
        self.status = format!("PLACED {callsign}");
        self.unsaved = true;
    }

    /// Step a battery row. False if nothing changed.
    fn step_fires(&mut self, f: Field, dir: i8, ds: &DataSet) -> bool {
        let s = self.side;
        if f == Field::Battery {
            let n = self.def.sides[s].fire_support.len();
            if n == 0 {
                self.status = "NO BATTERY: B PLACES ONE AT THE CURSOR".into();
                return false;
            }
            let k = self.battery_sel().map_or(0, |k| (k as i64 + i64::from(dir)).rem_euclid(n as i64) as usize);
            self.sel = Some(Sel::Battery(s, k));
            return false;
        }
        let Some(k) = self.battery_sel() else {
            self.status = "SELECT A BATTERY FIRST".into();
            return false;
        };
        let b = &mut self.def.sides[s].fire_support[k];
        let def = ds.fires.battery(&b.battery);
        let step = |cur: Option<u32>, stock: u32| {
            let v = i64::from(cur.unwrap_or(stock)) + i64::from(dir) * i64::from(ROUNDS_STEP);
            let v = v.clamp(0, 999) as u32;
            (v != stock).then_some(v)
        };
        match f {
            Field::BatteryType => {
                let list: Vec<&str> = ds.fires.batteries.iter().map(|b| b.id.as_str()).collect();
                if list.is_empty() {
                    return false;
                }
                b.battery = list[cycle(&list, &b.battery.as_str(), dir)].to_owned();
            }
            Field::BatteryHe => b.he_rounds = step(b.he_rounds, def.map_or(0, |d| d.he_rounds)),
            Field::BatterySmoke => b.smoke_rounds = step(b.smoke_rounds, def.map_or(0, |d| d.smoke_rounds)),
            _ => return false,
        }
        true
    }

    fn fires_value(&self, f: Field, ds: &DataSet) -> String {
        let Some(k) = self.battery_sel() else {
            return if f == Field::Battery {
                format!("{} OF THE SIDE", self.def.sides[self.side].fire_support.len())
            } else {
                "-".into()
            };
        };
        let b = &self.def.sides[self.side].fire_support[k];
        let def = ds.fires.battery(&b.battery);
        let rounds = |n: Option<u32>, stock: u32| n.map_or(format!("TYPE {stock}"), |n| n.to_string());
        match f {
            Field::Battery => b.callsign.clone(),
            Field::BatteryType => def.map_or(b.battery.to_uppercase(), |d| d.name.to_uppercase()),
            Field::BatteryHe => rounds(b.he_rounds, def.map_or(0, |d| d.he_rounds)),
            Field::BatterySmoke => rounds(b.smoke_rounds, def.map_or(0, |d| d.smoke_rounds)),
            _ => String::new(),
        }
    }

    fn fires_lines(&self, ds: &DataSet) -> Vec<(String, Rgba)> {
        let counts: Vec<String> =
            self.def.sides.iter().map(|s| format!("{} {}", s.name, s.fire_support.len())).collect();
        let mut v = vec![(format!("BATTERIES   {}", counts.join("   ")), style::TEXT_DIM)];
        let sel = self.battery_sel().and_then(|k| ds.fires.battery(&self.def.sides[self.side].fire_support[k].battery));
        match sel {
            Some(d) => v.push((
                format!(
                    "{} TUBES   RANGE {} - {}",
                    d.tubes,
                    format::distance(d.min_range_m.get()),
                    format::distance(d.max_range_m.get())
                ),
                style::TEXT_DIM,
            )),
            None => v.push(("B PLACES A BATTERY AT THE CURSOR".into(), style::TEXT_DIM)),
        }
        v
    }

    /// Batteries on the map (field artillery: frame with a filled dot); the selected one shows
    /// its minimum and maximum range.
    fn draw_batteries(&self, f: &mut Frame, cam: &Camera, ds: &DataSet) {
        for (si, side) in self.def.sides.iter().enumerate() {
            let c = if si == 0 { style::FRIENDLY } else { style::HOSTILE };
            for (k, b) in side.fire_support.iter().enumerate() {
                let p = cam.to_screen(b.position);
                symbols::field_artillery(f, p, c, si == 0);
                f.text(&b.callsign, p[0] + 18.0, p[1] - 6.0, style::FONT_L - 10, c);
                if self.sel == Some(Sel::Battery(si, k)) {
                    symbols::selection(f, p);
                    if let Some(d) = ds.fires.battery(&b.battery) {
                        for r in [d.min_range_m.get(), d.max_range_m.get()] {
                            f.circle_lines(p, (r / cam.mpp) as f32, style::TEXT_DIM);
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Victory conditions (`ScenarioDef.victory`, sim::victory).
// ---------------------------------------------------------------------------------------------

/// Time-limit step and range (s).
const VIC_TIME_STEP_S: f64 = 300.0;
const VIC_TIME_S: (f64, f64) = (600.0, 6.0 * 3600.0);
/// Loss-limit step and range (share of a side's vehicles).
const VIC_LOSS_STEP: f64 = 0.05;
const VIC_LOSS: (f64, f64) = (0.1, 1.0);

impl ScenarioEditor {
    fn victory_objective(&self) -> Option<&str> {
        self.def.victory.as_ref().and_then(|v| v.objective.as_deref())
    }

    /// Step a victory row. False if nothing changed.
    fn step_victory(&mut self, f: Field, dir: i8) -> bool {
        let d = f64::from(dir);
        if f == Field::VicOn {
            self.def.victory = match self.def.victory {
                Some(_) => None,
                None => Some(VictoryDef {
                    time_limit_s: 3600.0,
                    objective: self.def.objectives.first().map(|o| o.name.clone()),
                    attacker: 0,
                    destroyed_fraction: 0.7,
                }),
            };
            return true;
        }
        let names: Vec<Option<String>> =
            std::iter::once(None).chain(self.def.objectives.iter().map(|o| Some(o.name.clone()))).collect();
        let sides = self.def.sides.len();
        let Some(v) = &mut self.def.victory else {
            self.status = "VICTORY CONDITIONS ARE OFF".into();
            return false;
        };
        match f {
            Field::VicTime => {
                v.time_limit_s = (v.time_limit_s + VIC_TIME_STEP_S * d).clamp(VIC_TIME_S.0, VIC_TIME_S.1);
            }
            Field::VicObjective => v.objective = names[cycle(&names, &v.objective, dir)].clone(),
            Field::VicAttacker => {
                v.attacker = (v.attacker as i64 + i64::from(dir)).rem_euclid(sides.max(1) as i64) as usize;
            }
            Field::VicLoss => {
                let x = (v.destroyed_fraction + VIC_LOSS_STEP * d).clamp(VIC_LOSS.0, VIC_LOSS.1);
                // Keep the value on the 5 % grid (no drift from repeated float steps).
                v.destroyed_fraction = (x / VIC_LOSS_STEP).round() * VIC_LOSS_STEP;
            }
            _ => return false,
        }
        true
    }

    fn victory_value(&self, f: Field) -> String {
        let Some(v) = &self.def.victory else {
            return if f == Field::VicOn { "OFF".into() } else { "-".into() };
        };
        match f {
            Field::VicOn => "ON".into(),
            Field::VicTime => format!("{:.0} MIN", v.time_limit_s / 60.0),
            Field::VicObjective => v.objective.clone().unwrap_or_else(|| "NONE".into()),
            Field::VicAttacker => self.def.sides.get(v.attacker).map_or("-".into(), |s| s.name.clone()),
            Field::VicLoss => format!("{:.0} %", v.destroyed_fraction * 100.0),
            _ => String::new(),
        }
    }

    fn victory_lines(&self) -> Vec<(String, Rgba)> {
        let Some(v) = &self.def.victory else {
            return vec![("NO VICTORY CONDITIONS: RUNS UNTIL STOPPED".into(), style::TEXT_DIM)];
        };
        let end = format::clock(f64::from(self.def.start_time_s) + v.time_limit_s);
        let defender = self.def.sides.get((v.attacker + 1) % self.def.sides.len().max(1)).map_or("-", |s| &s.name);
        let mut out = vec![(format!("ENDS AT {end}; TIES GO TO {defender}"), style::TEXT_DIM)];
        if let Some(o) = &v.objective {
            out.push((format!("SEIZED: ATTACKER ALONE ON {o} FOR 2 MIN"), style::TEXT_DIM));
        }
        out
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
        ed.place_battery([3000.0, 2000.0], &ds);
        ed.act(EdAction::Step(Field::BatteryHe, -1), &ds);
        ed.act(EdAction::Step(Field::BatteryType, 1), &ds);
        ed.side = 1;
        ed.place_battery([5000.0, 6000.0], &ds);
        ed.act(EdAction::Step(Field::BatterySmoke, 1), &ds);
        assert_eq!(ed.def.sides[1].fire_support[0].callsign, "ARTY 2");
        ed.def.victory = None;
        for (f, d) in [
            (Field::VicOn, 1),
            (Field::VicTime, -1),
            (Field::VicObjective, 1),
            (Field::VicAttacker, 1),
            (Field::VicLoss, 1),
        ] {
            ed.act(EdAction::Step(f, d), &ds);
        }
        let v = ed.def.victory.as_ref().expect("victory on");
        assert_eq!((v.time_limit_s, v.attacker), (3300.0, 1));
        assert_eq!(v.destroyed_fraction, 0.75);
        assert!(ed.def.sides[0].fire_support[0].he_rounds.is_some());
        ed.prepare_save();
        DataSet::save_scenario(&tmp, &ed.def).unwrap();
        let back = DataSet::load_scenario(&tmp, &ed.def.id).unwrap();
        assert_eq!(dump(&back), dump(&ed.def));
        assert_ne!(back.weather, sim_data::WeatherDef::default());
        std::fs::remove_dir_all(&tmp).ok();
    }
}
