//! Game mode: runs the simulation, shows the side's perceived picture, issues orders.
//!
//! Only `PerceivedWorld` is read for display and selection (CLAUDE.md rule 3). Ground truth
//! is drawn only with the `debug-truth` feature.

use std::collections::VecDeque;
use std::sync::Arc;

use sim::{Command, EntityId, Event, PerceivedWorld, SideId, Sim, Task};
use sim_data::{DataSet, ScenarioDef};
use terrain::BakedMap;

use crate::format;
use crate::render::{Frame, Input, Key, P2};
use crate::style;
use crate::symbols;
use crate::view::Camera;

/// Time compression steps (DESIGN.md §2.1).
pub const SPEEDS: [u32; 4] = [1, 2, 4, 8];
/// Cap on ticks per rendered frame so a slow frame cannot spiral.
const MAX_TICKS_PER_FRAME: u32 = 40;

pub struct Game {
    pub sim: Sim,
    pub scenario: ScenarioDef,
    pub side: SideId,
    pub selected: Option<EntityId>,
    pub paused: bool,
    pub speed: usize,
    acc: f64,
    /// Own-unit positions/headings before the most recent tick, for interpolation.
    prev: Vec<(EntityId, [f64; 2], f64)>,
    pub log: VecDeque<(String, String)>,
}

impl Game {
    pub fn new(scenario: ScenarioDef, ds: Arc<DataSet>, map: Arc<BakedMap>) -> Result<Self, String> {
        let sim = Sim::new(&scenario, ds, map)?;
        let side = SideId(0);
        let mut g = Self {
            sim,
            scenario,
            side,
            selected: None,
            paused: true,
            speed: 0,
            acc: 0.0,
            prev: Vec::new(),
            log: VecDeque::new(),
        };
        g.selected = g.picture().own.first().map(|u| u.id);
        g.capture_prev();
        let name = g.scenario.name.clone();
        g.radio(format!("{name}. PAUSED. SPACE TO START."));
        Ok(g)
    }

    pub fn picture(&self) -> &PerceivedWorld {
        self.sim.perceived(self.side)
    }

    fn radio(&mut self, text: String) {
        self.log.push_back((format::clock(self.sim.clock_s()), text));
        while self.log.len() > 200 {
            self.log.pop_front();
        }
    }

    fn capture_prev(&mut self) {
        self.prev = self.picture().own.iter().map(|u| (u.id, u.pos, u.heading)).collect();
    }

    pub fn order_move(&mut self, unit: EntityId, dest: [f64; 2]) {
        self.sim.submit(self.side, Command::Move { unit, destination: dest, speed_limit_mps: None });
    }

    pub fn advance_ticks(&mut self, n: u32) {
        for _ in 0..n {
            self.capture_prev();
            let events = self.sim.step();
            for e in events {
                if let Some(t) = radio_text(&e, self.sim.perceived(self.side)) {
                    self.radio(t);
                }
            }
        }
    }

    pub fn update(&mut self, inp: &Input, cam: &Camera, over_ui: bool) {
        if inp.hit(Key::Space) {
            self.paused = !self.paused;
        }
        for (k, _) in SPEEDS.iter().enumerate() {
            if inp.hit(Key::Num(k as u8 + 1)) {
                self.speed = k;
            }
        }
        if inp.hit(Key::Tab) {
            let own = &self.picture().own;
            if !own.is_empty() {
                let i =
                    self.selected.and_then(|s| own.iter().position(|u| u.id == s)).map_or(0, |i| (i + 1) % own.len());
                self.selected = Some(own[i].id);
            }
        }
        if inp.hit(Key::Escape) {
            self.selected = None;
        }
        if inp.hit(Key::Char('h'))
            && let Some(u) = self.selected
        {
            self.sim.submit(self.side, Command::Halt { unit: u });
        }
        if !over_ui && inp.left_pressed {
            self.selected = self.pick(cam, inp.mouse);
        }
        if !over_ui
            && inp.right_pressed
            && let Some(u) = self.selected
        {
            let dest = cam.to_world(inp.mouse);
            self.order_move(u, dest);
        }
        if !self.paused {
            self.acc += inp.frame_time as f64 * SPEEDS[self.speed] as f64;
            let n = ((self.acc / sim::DT) as u32).min(MAX_TICKS_PER_FRAME);
            self.acc -= n as f64 * sim::DT;
            self.acc = self.acc.min(sim::DT * 2.0);
            self.advance_ticks(n);
        }
    }

    fn pick(&self, cam: &Camera, s: P2) -> Option<EntityId> {
        self.picture()
            .own
            .iter()
            .map(|u| {
                let p = cam.to_screen(u.pos);
                (u.id, (p[0] - s[0]).powi(2) + (p[1] - s[1]).powi(2))
            })
            .filter(|(_, d)| *d < 18.0 * 18.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _)| id)
    }

    fn interp(&self, id: EntityId, pos: [f64; 2], heading: f64) -> ([f64; 2], f64) {
        let a = if self.paused { 1.0 } else { (self.acc / sim::DT).clamp(0.0, 1.0) };
        match self.prev.iter().find(|p| p.0 == id) {
            Some(&(_, pp, ph)) => {
                let dh = ai_wrap(heading - ph);
                ([pp[0] + (pos[0] - pp[0]) * a, pp[1] + (pos[1] - pp[1]) * a], ph + dh * a)
            }
            None => (pos, heading),
        }
    }

    pub fn draw_world(&self, f: &mut Frame, cam: &Camera) {
        let pic = self.picture();
        // Routes: solid while executing (DESIGN.md §10.3).
        for u in &pic.own {
            if u.route.len() >= 2 {
                for w in u.route.windows(2) {
                    f.line(cam.to_screen(w[0]), cam.to_screen(w[1]), 1.5, style::ROUTE);
                }
                let end = cam.to_screen(*u.route.last().unwrap());
                f.circle_lines(end, 5.0, style::ROUTE);
            }
        }
        let zoomed = cam.mpp < 0.6;
        for u in &pic.own {
            let (pos, hdg) = self.interp(u.id, u.pos, u.heading);
            let p = cam.to_screen(pos);
            if zoomed {
                symbols::hull(f, cam, pos, hdg, 7.9, 3.7, style::FRIENDLY);
            }
            symbols::friendly_armour(f, p, style::FRIENDLY);
            if u.speed > 0.3 {
                symbols::heading_staff(f, p, hdg, 14.0, style::FRIENDLY);
            }
            f.text(&u.callsign, p[0] + 16.0, p[1] - 5.0, style::FONT, style::FRIENDLY);
            if self.selected == Some(u.id) {
                symbols::selection(f, p);
            }
        }
        #[cfg(feature = "debug-truth")]
        self.draw_truth(f, cam);
    }

    #[cfg(feature = "debug-truth")]
    fn draw_truth(&self, f: &mut Frame, cam: &Camera) {
        let w = self.sim.truth();
        for k in 0..w.len() {
            if w.side[k] != self.side {
                let p = cam.to_screen(w.transform[k].pos);
                symbols::hostile_armour(f, p, style::HOSTILE);
                f.text(&format!("{} [TRUTH]", w.callsign[k]), p[0] + 18.0, p[1] - 5.0, style::FONT, style::HOSTILE);
            }
        }
    }

    pub fn draw_ui(&self, f: &mut Frame, screen: P2) {
        // Top bar.
        let side = &self.sim.side_names()[self.side.0 as usize];
        let state = if self.paused { "PAUSED".to_owned() } else { format!("X{}", SPEEDS[self.speed]) };
        let top = format!("{}   {}   {}   {}", format::clock(self.sim.clock_s()), state, side, self.scenario.name);
        f.text(&top, 10.0, 6.0, style::FONT_L - 4, if self.paused { style::AMBER } else { style::TEXT });

        // Unit panel.
        let pw = 230.0;
        let (px, py) = (screen[0] - pw - 10.0, 40.0);
        f.rect(px, py, pw, 150.0, style::PANEL);
        f.rect_lines(px, py, pw, 150.0, style::PANEL_EDGE);
        if let Some(u) = self.selected.and_then(|id| self.picture().unit(id)) {
            let task = match &u.task {
                Task::Idle => "HALTED".to_owned(),
                Task::Moving { remaining_m } => format!("MOVING, {} TO GO", format::distance(*remaining_m)),
            };
            let lines = [
                format!("{}  {}", u.callsign, u.vehicle_name),
                format!("GRID  {}", format::grid8(u.pos)),
                format!("HDG   {} MILS", format::mils(u.heading)),
                format!("SPD   {} KM/H", format::kmh(u.speed)),
                format!("FUEL  {:.0} %", u.fuel_fraction * 100.0),
                format!("TASK  {task}"),
            ];
            for (k, l) in lines.iter().enumerate() {
                f.text(
                    l,
                    px + 10.0,
                    py + 10.0 + k as f32 * 20.0,
                    style::FONT_L - 6,
                    if k == 0 { style::FRIENDLY } else { style::TEXT },
                );
            }
        } else {
            f.text("NO UNIT SELECTED", px + 10.0, py + 10.0, style::FONT_L - 6, style::TEXT_DIM);
        }

        // Radio log.
        let n = 7;
        let (lw, lh) = (560.0, n as f32 * 16.0 + 12.0);
        let (lx, ly) = (10.0, screen[1] - lh - 28.0);
        f.rect(lx, ly, lw, lh, style::PANEL);
        f.rect_lines(lx, ly, lw, lh, style::PANEL_EDGE);
        for (k, (t, msg)) in self.log.iter().rev().take(n).collect::<Vec<_>>().into_iter().rev().enumerate() {
            f.text(&format!("{t}  {msg}"), lx + 8.0, ly + 6.0 + k as f32 * 16.0, style::FONT_L - 8, style::TEXT);
        }
    }

    /// Screen regions where map clicks should be ignored.
    pub fn over_ui(&self, s: P2, screen: P2) -> bool {
        let in_panel = s[0] > screen[0] - 240.0 && s[1] > 40.0 && s[1] < 190.0;
        let in_log = s[0] < 570.0 && s[1] > screen[1] - 7.0 * 16.0 - 40.0 && s[1] < screen[1] - 28.0;
        in_panel || in_log
    }
}

fn ai_wrap(a: f64) -> f64 {
    let t = std::f64::consts::TAU;
    let a = a.rem_euclid(t);
    if a > std::f64::consts::PI { a - t } else { a }
}

/// Terse radio text for an event (callsign, grid, distance; no flavour).
pub fn radio_text(e: &Event, pic: &PerceivedWorld) -> Option<String> {
    let u = pic.unit(e.unit())?;
    let cs = &u.callsign;
    let g = format::grid(u.pos);
    Some(match e {
        Event::RoutePlanned { length_m, .. } => {
            let dest = u.route.last().map_or(g.clone(), |&p| format::grid(p));
            format!("{cs}: MOVING TO GRID {dest}, {}.", format::distance(*length_m))
        }
        Event::NoRoute { .. } => format!("{cs}: UNABLE. NO ROUTE."),
        Event::Arrived { .. } => format!("{cs}: SET AT GRID {g}."),
        Event::Halted { .. } => format!("{cs}: HALTED AT GRID {g}."),
        Event::Stuck { .. } => format!("{cs}: IMMOBILE AT GRID {g}."),
        Event::OutOfFuel { .. } => format!("{cs}: BLACK ON FUEL AT GRID {g}."),
    })
}
