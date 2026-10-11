//! After-action review (DESIGN.md §10.3): ground truth is allowed here. Truth snapshots are
//! recorded once per second during play; shots and reports come from events. The AAR shows a
//! timeline scrubber, every unit of every side, shot lines, and per-side statistics.

use std::collections::BTreeMap;

use sim::view::{Condition, EntityId, SideId};
use sim::{Event, Sim};

use crate::format;
use crate::render::{Frame, Input, Key, P2, Rgba};
use crate::style;
use crate::symbols;
use crate::view::Camera;

#[derive(Debug, Clone)]
pub struct UnitSnap {
    pub id: EntityId,
    pub side: SideId,
    pub pos: [f64; 2],
    pub heading: f64,
    pub condition: Condition,
}

#[derive(Debug, Clone)]
pub struct Frame1s {
    pub t: f64,
    pub units: Vec<UnitSnap>,
}

#[derive(Debug, Clone)]
pub struct Shot {
    pub t: f64,
    pub side: SideId,
    pub from: [f64; 2],
    pub to: [f64; 2],
    pub hit: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SideStats {
    pub first_report_s: Option<f64>,
    pub rounds: u32,
    pub hits: u32,
    pub kills: u32,
    pub losses: u32,
    /// Range band (500 m) → (shots, hits).
    pub by_range: BTreeMap<u32, (u32, u32)>,
}

#[derive(Default)]
pub struct Recorder {
    pub frames: Vec<Frame1s>,
    pub shots: Vec<Shot>,
    pub stats: Vec<SideStats>,
    pub callsigns: BTreeMap<EntityId, String>,
}

impl Recorder {
    pub fn observe(&mut self, sim: &Sim, events: &[Event]) {
        let w = sim.truth();
        let n_sides = sim.side_names().len();
        if self.stats.len() != n_sides {
            self.stats = vec![SideStats::default(); n_sides];
        }
        let t = sim.time_s();
        let side_of = |id: EntityId| w.index_of(id).map(|k| w.side[k]);
        for e in events {
            match e {
                Event::Fired { unit, range_m, from, to, hit } => {
                    if let Some(s) = side_of(*unit) {
                        let st = &mut self.stats[s.0 as usize];
                        st.rounds += 1;
                        if *hit {
                            st.hits += 1;
                        }
                        let band = (*range_m / 500.0) as u32;
                        let b = st.by_range.entry(band).or_default();
                        b.0 += 1;
                        if *hit {
                            b.1 += 1;
                        }
                        self.shots.push(Shot { t, side: s, from: *from, to: *to, hit: *hit });
                    }
                }
                Event::Hit { unit, kill: Some(sim::KillKind::Catastrophic), .. } => {
                    if let Some(s) = side_of(*unit) {
                        self.stats[s.0 as usize].losses += 1;
                        let other = (s.0 as usize + 1) % n_sides;
                        self.stats[other].kills += 1;
                    }
                }
                Event::SpotReport { side, .. } => {
                    let st = &mut self.stats[side.0 as usize];
                    st.first_report_s.get_or_insert(t);
                }
                _ => {}
            }
        }
        if sim.tick().is_multiple_of(sim::TICK_HZ as u64) {
            if self.callsigns.is_empty() {
                for k in 0..w.len() {
                    self.callsigns.insert(w.id[k], w.callsign[k].clone());
                }
            }
            self.frames.push(Frame1s {
                t,
                units: (0..w.len())
                    .map(|k| UnitSnap {
                        id: w.id[k],
                        side: w.side[k],
                        pos: w.transform[k].pos,
                        heading: w.transform[k].heading,
                        condition: w.condition[k],
                    })
                    .collect(),
            });
        }
    }
}

pub struct AarView {
    pub index: usize,
    pub playing: bool,
    acc: f32,
}

impl AarView {
    pub fn new(rec: &Recorder) -> Self {
        Self { index: rec.frames.len().saturating_sub(1), playing: false, acc: 0.0 }
    }

    fn bar(screen: P2) -> (f32, f32, f32) {
        (40.0, screen[0] - 380.0, screen[1] - 70.0)
    }

    pub fn update(&mut self, inp: &Input, rec: &Recorder) {
        let n = rec.frames.len();
        if n == 0 {
            return;
        }
        if inp.hit(Key::Space) {
            self.playing = !self.playing;
        }
        if inp.hit(Key::Left) {
            self.index = self.index.saturating_sub(10);
        }
        if inp.hit(Key::Right) {
            self.index = (self.index + 10).min(n - 1);
        }
        let (x0, x1, y) = Self::bar(inp.screen);
        if inp.left_down
            && inp.mouse[1] > y - 12.0
            && inp.mouse[1] < y + 12.0
            && inp.mouse[0] >= x0
            && inp.mouse[0] <= x1
        {
            let f = (inp.mouse[0] - x0) / (x1 - x0);
            self.index = ((f * (n - 1) as f32).round() as usize).min(n - 1);
        }
        if self.playing {
            self.acc += inp.frame_time * 10.0; // ×10 playback
            while self.acc >= 1.0 {
                self.acc -= 1.0;
                if self.index + 1 < n {
                    self.index += 1;
                } else {
                    self.playing = false;
                }
            }
        }
    }

    pub fn draw_world(&self, f: &mut Frame, cam: &Camera, rec: &Recorder) {
        let Some(fr) = rec.frames.get(self.index) else { return };
        for s in rec.shots.iter().filter(|s| s.t <= fr.t && fr.t - s.t < 6.0) {
            let a = (255.0 * (1.0 - (fr.t - s.t) / 6.0)) as u8;
            let c = if s.side.0 == 0 { Rgba(150, 210, 240, a) } else { Rgba(240, 140, 120, a) };
            f.line(cam.to_screen(s.from), cam.to_screen(s.to), 1.2, c);
            if s.hit {
                f.circle(cam.to_screen(s.to), 3.0, Rgba(250, 230, 160, a));
            }
        }
        for u in &fr.units {
            let p = cam.to_screen(u.pos);
            if u.side.0 == 0 {
                symbols::friendly_armour(f, p, style::FRIENDLY);
            } else {
                symbols::hostile_armour(f, p, style::HOSTILE);
            }
            match u.condition {
                Condition::Destroyed => symbols::destroyed(f, p, Rgba(220, 210, 190, 255)),
                Condition::MobilityKill | Condition::FirepowerKill => symbols::damaged(f, p, style::AMBER),
                Condition::Operational => symbols::heading_staff(f, p, u.heading, 8.0, Rgba(200, 200, 200, 160)),
            }
            if let Some(cs) = rec.callsigns.get(&u.id) {
                f.text(
                    cs,
                    p[0] + 16.0,
                    p[1] - 5.0,
                    style::FONT,
                    if u.side.0 == 0 { style::FRIENDLY } else { style::HOSTILE },
                );
            }
        }
    }

    pub fn draw_ui(&self, f: &mut Frame, screen: P2, rec: &Recorder, names: &[String], start_clock: f64) {
        let n = rec.frames.len();
        let (x0, x1, y) = Self::bar(screen);
        f.rect(x0 - 10.0, y - 22.0, x1 - x0 + 20.0, 44.0, style::PANEL);
        f.line([x0, y], [x1, y], 2.0, style::TEXT_DIM);
        if n > 1 {
            let x = x0 + (x1 - x0) * self.index as f32 / (n - 1) as f32;
            f.rect(x - 3.0, y - 9.0, 6.0, 18.0, style::AMBER);
            // Kill ticks on the timeline.
            let tmax = rec.frames[n - 1].t.max(1.0);
            for s in rec.shots.iter().filter(|s| s.hit) {
                let xs = x0 + (x1 - x0) * (s.t / tmax) as f32;
                f.line(
                    [xs, y + 4.0],
                    [xs, y + 10.0],
                    1.0,
                    if s.side.0 == 0 { style::FRIENDLY } else { style::HOSTILE },
                );
            }
        }
        let t = rec.frames.get(self.index).map_or(0.0, |f| f.t);
        let head = format!(
            "AFTER-ACTION REVIEW  {}  (H+{:.0}:{:02.0})  {}   SPACE PLAY  LEFT/RIGHT 10 S  DRAG TIMELINE  CTRL+R SAVE REPLAY  F8 EXIT",
            format::clock(start_clock + t),
            (t / 60.0).floor(),
            t % 60.0,
            if self.playing { "PLAYING X10" } else { "" }
        );
        f.text(&head, 10.0, 6.0, style::FONT_L - 6, style::AMBER);
        // Statistics panel.
        let (px, py, pw) = (screen[0] - 330.0, 40.0, 320.0);
        let mut lines: Vec<(String, Rgba)> = Vec::new();
        for (i, s) in rec.stats.iter().enumerate() {
            let c = if i == 0 { style::FRIENDLY } else { style::HOSTILE };
            lines.push((names.get(i).cloned().unwrap_or_default(), c));
            lines.push((
                format!("  FIRST CONTACT REPORT  {}", s.first_report_s.map_or("-".into(), |t| format!("H+{:.0} S", t))),
                style::TEXT,
            ));
            lines.push((
                format!("  ROUNDS {}  HITS {}  KILLS {}  LOSSES {}", s.rounds, s.hits, s.kills, s.losses),
                style::TEXT,
            ));
            let bands: Vec<String> = s
                .by_range
                .iter()
                .map(|(b, (sh, h))| format!("{:.1}KM {}/{}", (*b as f64 + 0.5) * 0.5, h, sh))
                .collect();
            if !bands.is_empty() {
                lines.push((format!("  HITS BY RANGE {}", bands.join(" ")), style::TEXT_DIM));
            }
        }
        let ph = lines.len() as f32 * 18.0 + 16.0;
        f.rect(px, py, pw, ph, style::PANEL);
        f.rect_lines(px, py, pw, ph, style::PANEL_EDGE);
        for (k, (l, c)) in lines.iter().enumerate() {
            f.text(l, px + 8.0, py + 8.0 + k as f32 * 18.0, style::FONT_L - 9, *c);
        }
    }
}
