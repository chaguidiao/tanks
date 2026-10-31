//! Menu screens and overlays (DESIGN.md §10.4): title, main menu, scenario and map select with
//! briefing, loading, options, the screen menu (Esc) and the unsaved-changes prompt. Buttons are
//! `hud::Button`s laid out per frame like the rest of the HUD; menus also take Up/Down/Enter.

use sim_data::{MapRef, ScenarioDef, VehicleClass};
use terrain::contour::{self, ContourKind, ContourSpec};
use terrain::{Grid, Heightfield};

use crate::format;
use crate::hud::{self, Button, Look, Rect};
use crate::mapscreen::ScreenAction;
use crate::render::{Frame, Input, Key, P2, Rgba, Tex};
use crate::settings::Settings;
use crate::style;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opt {
    Fullscreen,
    Fps,
    Font,
    Pan,
    EdgePan,
    Contours,
    Grid,
    PauseOnMenu,
    PauseOnReady,
    AutoPause,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    /// Main menu: back to the suspended screen; screen menu: close the menu.
    Resume,
    Play,
    EditScenario,
    EditTerrain,
    /// Vehicle / ammunition / terrain / doctrine data viewer.
    Database,
    Options,
    Help,
    Quit,
    /// List row.
    Pick(usize),
    /// Open the picked scenario or map.
    Start,
    Back,
    Opt(Opt),
    Defaults,
    /// Screen-menu entry run by the map screen.
    Screen(ScreenAction),
    MainMenu,
    /// Leave despite unsaved edits.
    Discard,
}

/// What a scenario list opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    Play,
    Edit,
}

pub struct ScenarioEntry {
    pub id: String,
    pub def: Result<ScenarioDef, String>,
    /// Per side, the force by vehicle class (`8 TANKS · 3 IFVS`), see `force_summary`.
    pub forces: Vec<String>,
}

/// Count a side's vehicles by class, e.g. `8 TANKS · 3 IFVS`. Classes appear in the order
/// MBT, IFV, RECON, SUPPORT; a vehicle id the data set does not know counts as `UNKNOWN`.
pub fn force_summary<'a>(
    vehicles: impl IntoIterator<Item = &'a str>,
    class_of: impl Fn(&str) -> Option<VehicleClass>,
) -> String {
    let mut counts = [0usize; 5];
    for v in vehicles {
        counts[match class_of(v) {
            Some(VehicleClass::Mbt) => 0,
            Some(VehicleClass::Ifv) => 1,
            Some(VehicleClass::Recon) => 2,
            Some(VehicleClass::Support) => 3,
            None => 4,
        }] += 1;
    }
    let names = [("TANK", "TANKS"), ("IFV", "IFVS"), ("RECON", "RECON"), ("TRUCK", "TRUCKS"), ("UNKNOWN", "UNKNOWN")];
    let parts: Vec<String> = counts
        .iter()
        .zip(names)
        .filter(|(n, _)| **n > 0)
        .map(|(n, (one, many))| format!("{n} {}", if *n == 1 { one } else { many }))
        .collect();
    if parts.is_empty() { "NO VEHICLES".into() } else { parts.join(" · ") }
}

pub struct ScenarioList {
    pub purpose: Purpose,
    pub entries: Vec<ScenarioEntry>,
    pub sel: usize,
}

pub struct MapEntry {
    pub map: MapRef,
    pub generated: bool,
    /// Names of the scenarios played on it.
    pub used_by: Vec<String>,
}

pub struct MapList {
    pub entries: Vec<MapEntry>,
    pub sel: usize,
}

const ROW_H: f32 = 38.0;
const ROW_GAP: f32 = 6.0;
const LIST_ROW_H: f32 = 30.0;

fn menu_button(r: Rect, label: &str, a: MenuAction, key: &'static str) -> Button<MenuAction> {
    Button::new(r, Look::Menu, label, a).key(key)
}

/// Lay menu entries out as a column from (x, y).
fn column(x: f32, y: f32, w: f32, items: Vec<(String, MenuAction, &'static str)>) -> Vec<Button<MenuAction>> {
    items
        .into_iter()
        .enumerate()
        .map(|(k, (l, a, key))| menu_button(Rect::new(x, y + k as f32 * (ROW_H + ROW_GAP), w, ROW_H), &l, a, key))
        .collect()
}

/// Keyboard and mouse focus over `Look::Menu` buttons: hover moves the focus, Up/Down cycle it,
/// Enter activates it. Marks the focused button and returns the activated action.
pub fn nav(buttons: &mut [Button<MenuAction>], focus: &mut Option<usize>, inp: &Input) -> Option<MenuAction> {
    let cands: Vec<usize> =
        (0..buttons.len()).filter(|&k| buttons[k].look == Look::Menu && buttons[k].enabled).collect();
    if cands.is_empty() {
        *focus = None;
        return None;
    }
    if (inp.mouse_delta[0] != 0.0 || inp.mouse_delta[1] != 0.0)
        && let Some(k) = cands.iter().copied().find(|&k| buttons[k].r.contains(inp.mouse))
    {
        *focus = Some(k);
    }
    let pos = focus.and_then(|f| cands.iter().position(|&k| k == f));
    if inp.hit(Key::Down) {
        *focus = Some(cands[pos.map_or(0, |p| (p + 1) % cands.len())]);
    }
    if inp.hit(Key::Up) {
        *focus = Some(cands[pos.map_or(cands.len() - 1, |p| (p + cands.len() - 1) % cands.len())]);
    }
    if !focus.is_some_and(|f| cands.contains(&f)) {
        *focus = Some(cands[0]);
    }
    for (k, b) in buttons.iter_mut().enumerate() {
        b.focused = Some(k) == *focus;
    }
    if inp.hit(Key::Enter) { focus.map(|f| buttons[f].action) } else { None }
}

// ---------------------------------------------------------------- map previews

/// Small relief images of maps already on disk, made on first use. A map not yet generated has
/// none (a preview would mean generating it).
#[derive(Default)]
pub struct Previews {
    cache: std::collections::BTreeMap<String, Option<Tex>>,
}

impl Previews {
    const MAX_PX: usize = 256;

    /// Build the preview of `id` if it has not been tried yet (needs the window, so it runs
    /// in the update step, not while drawing).
    pub fn ensure(&mut self, gfx: &mut crate::render::Gfx, data_dir: &std::path::Path, id: &str) {
        if !self.cache.contains_key(id) {
            let tex = Self::render(data_dir, id).map(|(w, h, px)| gfx.texture(w, h, &px));
            self.cache.insert(id.to_owned(), tex);
        }
    }

    /// Forget everything (a map may have been edited or generated since).
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    fn render(data_dir: &std::path::Path, id: &str) -> Option<(usize, usize, Vec<u8>)> {
        let dir = terragen::workflow::map_dir(data_dir, id);
        if !dir.join("map.toml").exists() {
            return None;
        }
        let hf = terrain::package::AuthoringPackage::load(&dir).ok()?.edited_elevation();
        let tint = terrain::shade::elevation_tint(&hf);
        let shade = terrain::shade::hillshade(&hf);
        let (w, h) = (hf.z.w, hf.z.h);
        let step = w.max(h).div_ceil(Self::MAX_PX).max(1);
        let (ow, oh) = (w / step, h / step);
        let mut out = vec![255u8; ow * oh * 4];
        for y in 0..oh {
            for x in 0..ow {
                let (a, b) = ((y * step * w + x * step) * 4, (y * ow + x) * 4);
                let sh = shade[a] as f32 / 200.0;
                for c in 0..3 {
                    out[b + c] = (tint[a + c] as f32 * sh).min(255.0) as u8;
                }
            }
        }
        Some((ow, oh, out))
    }

    /// Draw the preview (or a placeholder) in a square at `r`, with a frame.
    fn draw(&self, f: &mut Frame, id: &str, r: Rect) {
        match self.cache.get(id).and_then(Option::as_ref) {
            Some(t) => {
                let side = r.w.min(r.h);
                let (tw, th) = (t.w as f32, t.h as f32);
                let k = (side / tw).min(side / th);
                let (w, h) = (tw * k, th * k);
                f.texture(t, r.x, r.y, w, h, Rgba(255, 255, 255, 255));
                f.rect_lines(r.x, r.y, w, h, style::PANEL_EDGE);
            }
            None => {
                f.rect_lines(r.x, r.y, r.w, r.h, style::PANEL_EDGE);
                f.text("NO PREVIEW", r.x + 12.0, r.y + 12.0, style::FONT_S, style::TEXT_OFF);
                f.text("MAP NOT GENERATED", r.x + 12.0, r.y + 32.0, style::FONT_S, style::TEXT_OFF);
            }
        }
    }
}

// ---------------------------------------------------------------- backdrop

/// Faint contour pattern behind the menus: a small procedural heightfield, contoured once.
pub struct Backdrop {
    lines: Vec<(bool, Vec<[f32; 2]>)>,
    size: [f32; 2],
}

impl Backdrop {
    pub fn new() -> Self {
        let (w, h, cell) = (200, 120, 10.0);
        let mut z = Grid::new(w, h, 0.0f32);
        for j in 0..h {
            for i in 0..w {
                let (x, y) = (i as f64 * cell, j as f64 * cell);
                let v = terrain::noise::fbm(x / 700.0, y / 700.0, 1985, 5, 2.0, 0.5);
                z.data[j * w + i] = (v * 90.0) as f32;
            }
        }
        let hf = Heightfield::new(z, cell);
        let spec = ContourSpec { interval: 6.0, intermediate_every: 30.0, index_every: 30.0, simplify_m: 3.0 };
        let lines = contour::extract(&hf, &spec)
            .into_iter()
            .map(|c| {
                let mut pts = c.points;
                if c.closed && !pts.is_empty() {
                    pts.push(pts[0]);
                }
                (matches!(c.kind, ContourKind::Index), pts)
            })
            .collect();
        Self { lines, size: [hf.width_m() as f32, hf.height_m() as f32] }
    }

    pub fn draw(&self, f: &mut Frame, screen: P2) {
        f.clear(style::BG);
        // Cover the screen with a uniform scale (y up in metres, down on screen).
        let s = (screen[0] / self.size[0]).max(screen[1] / self.size[1]);
        let to = |p: [f32; 2]| [p[0] * s, screen[1] - p[1] * s];
        for (index, pts) in &self.lines {
            let c = if *index { Rgba(70, 76, 62, 150) } else { Rgba(52, 58, 50, 120) };
            for w in pts.windows(2) {
                f.line(to(w[0]), to(w[1]), 1.0, c);
            }
        }
        // 1 km-style grid, as on the map.
        let step = 160.0;
        let mut x = 0.0;
        while x < screen[0] {
            f.line([x, 0.0], [x, screen[1]], 1.0, Rgba(120, 150, 130, 22));
            x += step;
        }
        let mut y = 0.0;
        while y < screen[1] {
            f.line([0.0, y], [screen[0], y], 1.0, Rgba(120, 150, 130, 22));
            y += step;
        }
    }
}

// ---------------------------------------------------------------- shared pieces

fn backend() -> &'static str {
    if cfg!(feature = "gl21") { "OPENGL 2.1" } else { "OPENGL 3.3" }
}

/// Version as shown to the player: the Cargo version for a tagged release build, with the build
/// id appended for any other packaged build (`tools/package.sh` sets `TANKS_BUILD` from
/// `git describe`), and DEV for a plain `cargo build`.
fn version_label(version: &str, build: Option<&str>) -> String {
    match build.map(|b| b.trim_start_matches('v')).filter(|b| !b.is_empty()) {
        None => format!("{version} DEV"),
        Some(b) if b == version => version.to_owned(),
        // "0.1.0-3-g1a2b3c4": commits after the release tag.
        Some(b) if b.starts_with(version) => b.to_uppercase(),
        Some(b) => format!("{version}+{}", b.to_uppercase()),
    }
}

fn footer(f: &mut Frame, screen: P2) {
    let v = version_label(env!("CARGO_PKG_VERSION"), option_env!("TANKS_BUILD"));
    let l = format!("TANKS {v}   ·   {}   ·   MIT LICENCE", backend());
    f.text(&l, 24.0, screen[1] - 28.0, style::FONT_S, style::TEXT_OFF);
}

pub fn heading(f: &mut Frame, x: f32, y: f32, text: &str, sub: &str) {
    f.text(text, x, y, style::FONT_XL, style::AMBER);
    f.text(sub, x, y + 42.0, style::FONT_S, style::TEXT_DIM);
}

/// Greedy word wrap to a pixel width; blank lines between paragraphs are kept.
pub fn wrap(f: &Frame, text: &str, size: i32, w: f32) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.split('\n') {
        let mut cur = String::new();
        for word in para.split_whitespace() {
            let cand = if cur.is_empty() { word.to_owned() } else { format!("{cur} {word}") };
            if !cur.is_empty() && f.text_width(&cand, size) > w {
                out.push(std::mem::replace(&mut cur, word.to_owned()));
            } else {
                cur = cand;
            }
        }
        out.push(cur);
    }
    out
}

fn panel(f: &mut Frame, r: Rect) {
    hud::panel(f, r);
}

/// Centred modal panel of the given size.
fn modal(screen: P2, w: f32, h: f32) -> Rect {
    Rect::new(((screen[0] - w) / 2.0).round(), ((screen[1] - h) / 2.0).round(), w, h)
}

pub fn draw_buttons<A>(f: &mut Frame, buttons: &[Button<A>], mouse: P2, screen: P2) {
    hud::draw(f, buttons, mouse);
    hud::tooltip(f, buttons, mouse, screen);
}

// ---------------------------------------------------------------- title

pub fn draw_title(f: &mut Frame, bd: &Backdrop, screen: P2, t: f32) {
    bd.draw(f, screen);
    let title = "T A N K S";
    let tw = f.text_width(title, style::FONT_TITLE);
    let y = (screen[1] * 0.36).round();
    f.text(title, (screen[0] - tw) / 2.0, y, style::FONT_TITLE, style::TEXT);
    let sub = "TANK COMMAND SIMULATION";
    let sw = f.text_width(sub, style::FONT_M);
    f.text(sub, (screen[0] - sw) / 2.0, y + 66.0, style::FONT_M, style::AMBER);
    let rule = 260.0;
    f.line([(screen[0] - rule) / 2.0, y + 56.0], [(screen[0] + rule) / 2.0, y + 56.0], 1.0, style::PANEL_EDGE);
    let line = "PLATOONS · DOCTRINE · TERRAIN · FOG OF WAR";
    let lw = f.text_width(line, style::FONT_S);
    f.text(line, (screen[0] - lw) / 2.0, y + 96.0, style::FONT_S, style::TEXT_DIM);
    // Slow, steady blink (1 s period): the only motion on the screen.
    if (t % 1.2) < 0.8 {
        let p = "PRESS ANY KEY";
        let pw = f.text_width(p, style::FONT_S);
        f.text(p, (screen[0] - pw) / 2.0, screen[1] * 0.72, style::FONT_S, style::TEXT);
    }
    footer(f, screen);
}

// ---------------------------------------------------------------- main menu

/// Left column x and width of the main menu.
fn main_col(screen: P2) -> (f32, f32, f32) {
    let x = (screen[0] * 0.08).max(40.0).round();
    (x, (screen[1] * 0.36).round(), 380.0)
}

pub fn main_buttons(screen: P2, suspended: Option<&str>) -> Vec<Button<MenuAction>> {
    let (x, y, w) = main_col(screen);
    let mut items = Vec::new();
    if let Some(s) = suspended {
        items.push((format!("RESUME  {}", short(s, 24)), MenuAction::Resume, ""));
    }
    items.extend([
        ("PLAY SCENARIO".to_owned(), MenuAction::Play, ""),
        ("SCENARIO EDITOR".to_owned(), MenuAction::EditScenario, ""),
        ("TERRAIN EDITOR".to_owned(), MenuAction::EditTerrain, ""),
        ("DATABASE".to_owned(), MenuAction::Database, ""),
        ("OPTIONS".to_owned(), MenuAction::Options, ""),
        ("QUIT".to_owned(), MenuAction::Quit, ""),
    ]);
    let tips = [
        "CHOOSE A SCENARIO, READ THE BRIEFING, TAKE COMMAND",
        "PLACE UNITS AND OBJECTIVES, SET DATE AND TIME, PLAY TEST",
        "SHAPE THE GROUND: HEIGHT BRUSHES, RIVERS, ROADS, FORESTS, BUILDINGS",
        "VEHICLES, AMMUNITION, TERRAIN, DOCTRINE AND COMBAT MODELS, WITH SOURCES",
        "DISPLAY, CONTROLS AND MAP DEFAULTS",
        "EXIT TO DESKTOP",
    ];
    let mut b = column(x, y, w, items);
    let n = b.len();
    for (k, t) in tips.iter().enumerate() {
        b[n - tips.len() + k].tip = (*t).to_owned();
    }
    b
}

fn short(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_owned() } else { format!("{}…", s.chars().take(n - 1).collect::<String>()) }
}

pub fn draw_main(f: &mut Frame, bd: &Backdrop, screen: P2, status: &str) {
    bd.draw(f, screen);
    let (x, y, _) = main_col(screen);
    f.text("T A N K S", x, y - 150.0, style::FONT_TITLE, style::TEXT);
    f.text("TANK COMMAND SIMULATION", x + 4.0, y - 86.0, style::FONT_M, style::AMBER);
    f.line([x + 4.0, y - 30.0], [x + 380.0, y - 30.0], 1.0, style::PANEL_EDGE);
    if !status.is_empty() {
        f.text(status, x, y + 7.0 * (ROW_H + ROW_GAP) + 16.0, style::FONT_S, style::HOSTILE);
    }
    let hint = "UP/DOWN SELECT   ENTER OPEN";
    f.text(hint, x, screen[1] - 56.0, style::FONT_S, style::TEXT_DIM);
    footer(f, screen);
}

// ---------------------------------------------------------------- list screens

fn list_frame(screen: P2) -> Rect {
    Rect::new(40.0, 140.0, 440.0, screen[1] - 140.0 - 100.0)
}

fn detail_frame(screen: P2) -> Rect {
    let l = list_frame(screen);
    let x = l.x + l.w + 16.0;
    Rect::new(x, l.y, (screen[0] - 40.0 - x).max(200.0), l.h)
}

/// List rows, BACK and the confirm button.
fn list_buttons(screen: P2, rows: Vec<(String, String)>, sel: usize, go: &str, go_ok: bool) -> Vec<Button<MenuAction>> {
    let l = list_frame(screen);
    let mut out: Vec<Button<MenuAction>> = rows
        .into_iter()
        .enumerate()
        .map(|(k, (label, value))| {
            let r = Rect::new(l.x + 8.0, l.y + 34.0 + k as f32 * (LIST_ROW_H + 4.0), l.w - 16.0, LIST_ROW_H);
            Button::new(r, Look::Row, label, MenuAction::Pick(k)).value(value).active(k == sel)
        })
        .filter(|b| b.r.y + b.r.h <= l.y + l.h)
        .collect();
    let y = screen[1] - 80.0;
    out.push(menu_button(Rect::new(40.0, y, 200.0, ROW_H), "BACK", MenuAction::Back, "ESC"));
    let d = detail_frame(screen);
    out.push(menu_button(Rect::new(d.x + d.w - 280.0, y, 280.0, ROW_H), go, MenuAction::Start, "ENTER").enabled(go_ok));
    out
}

/// Up/Down move the list selection; Enter is the confirm button (handled by the caller).
pub fn list_keys(inp: &Input, sel: &mut usize, n: usize) {
    if n == 0 {
        return;
    }
    if inp.hit(Key::Down) {
        *sel = (*sel + 1) % n;
    }
    if inp.hit(Key::Up) {
        *sel = (*sel + n - 1) % n;
    }
}

impl ScenarioList {
    pub fn buttons(&self, screen: P2) -> Vec<Button<MenuAction>> {
        let rows = self
            .entries
            .iter()
            .map(|e| match &e.def {
                Ok(d) => (d.name.to_uppercase(), format!("{}", d.date.0)),
                Err(_) => (e.id.to_uppercase(), "ERROR".into()),
            })
            .collect();
        let ok = self.entries.get(self.sel).is_some_and(|e| e.def.is_ok());
        let go = if self.purpose == Purpose::Play { "START" } else { "EDIT SCENARIO" };
        list_buttons(screen, rows, self.sel, go, ok)
    }

    pub fn draw(&self, f: &mut Frame, bd: &Backdrop, screen: P2, pv: &Previews) {
        bd.draw(f, screen);
        let (t, sub) = match self.purpose {
            Purpose::Play => ("SELECT SCENARIO", "YOU COMMAND THE FIRST SIDE; THE OTHER IS THE COMPUTER"),
            Purpose::Edit => ("SCENARIO EDITOR", "PICK A SCENARIO TO EDIT; SAVING WRITES <ID>-CUSTOM"),
        };
        heading(f, 40.0, 50.0, t, sub);
        let l = list_frame(screen);
        panel(f, l);
        f.text(&format!("SCENARIOS ({})", self.entries.len()), l.x + 12.0, l.y + 10.0, style::FONT, style::TEXT_DIM);
        if self.entries.is_empty() {
            f.text("NO SCENARIOS IN DATA/SCENARIOS", l.x + 12.0, l.y + 40.0, style::FONT_S, style::HOSTILE);
        }
        let d = detail_frame(screen);
        panel(f, d);
        let Some(e) = self.entries.get(self.sel) else { return };
        let (x, mut y, w) = (d.x + 20.0, d.y + 18.0, d.w - 40.0);
        let def = match &e.def {
            Ok(def) => def,
            Err(err) => {
                f.text(&e.id.to_uppercase(), x, y, style::FONT_L, style::HOSTILE);
                for l in wrap(f, &format!("CANNOT LOAD: {err}"), style::FONT_S, w) {
                    y += 20.0;
                    f.text(&l, x, y + 20.0, style::FONT_S, style::HOSTILE);
                }
                return;
            }
        };
        f.text(&def.name.to_uppercase(), x, y, style::FONT_L, style::AMBER);
        f.text(&def.id, x, y + 26.0, style::FONT_S, style::TEXT_OFF);
        y += 56.0;
        let mut facts = vec![
            (
                "DATE",
                format!(
                    "{:04}-{:02}-{:02}   START {}",
                    def.date.0,
                    def.date.1,
                    def.date.2,
                    format::clock(def.start_time_s as f64)
                ),
            ),
            (
                "MAP",
                format!(
                    "{}   {} · SEED {} · {:.0} KM",
                    def.map.id.to_uppercase(),
                    def.map.recipe.to_uppercase(),
                    def.map.seed,
                    def.map.size_m / 1000.0
                ),
            ),
        ];
        for (k, s) in def.sides.iter().enumerate() {
            let role = if k == 0 {
                "PLAYER"
            } else if s.ai.is_some() {
                "COMPUTER (PLAN)"
            } else {
                "COMPUTER"
            };
            facts.push((
                if k == 0 { "SIDES" } else { "" },
                format!(
                    "{} · {} · {} · {}",
                    s.name,
                    e.forces.get(k).map_or("", String::as_str),
                    s.doctrine.to_uppercase(),
                    role
                ),
            ));
        }
        let objs: Vec<String> = def.objectives.iter().map(|o| o.name.to_uppercase()).collect();
        facts.push(("OBJECTIVES", if objs.is_empty() { "NONE".into() } else { objs.join(", ") }));
        facts.push((
            "VICTORY",
            match &def.victory {
                Some(v) => format!(
                    "{:.0} MIN LIMIT · {:.0} % LOSSES LOSE{}",
                    v.time_limit_s / 60.0,
                    v.destroyed_fraction * 100.0,
                    v.objective.as_ref().map_or(String::new(), |o| format!(" · HOLD {}", o.to_uppercase()))
                ),
                None => "OPEN-ENDED (NO CONDITIONS)".into(),
            },
        ));
        for (k, v) in facts {
            f.text(k, x, y, style::FONT_S, style::TEXT_DIM);
            f.text(&v, x + 120.0, y, style::FONT_S, style::TEXT);
            y += 22.0;
        }
        y += 14.0;
        f.line([x, y], [x + w, y], 1.0, style::PANEL_EDGE);
        y += 16.0;
        f.text("BRIEFING", x, y, style::FONT, style::TEXT_DIM);
        y += 22.0;
        let side = (d.y + d.h - 24.0 - y).clamp(0.0, 300.0);
        if side > 120.0 {
            pv.draw(f, &def.map.id, Rect::new(x + w - side, y, side, side));
        }
        let text = def.briefing.as_deref().unwrap_or("NO BRIEFING. SEE THE SCENARIO FILE FOR NOTES.");
        let bw = if side > 120.0 { w - side - 20.0 } else { w };
        for l in wrap(f, text, style::FONT_S, bw) {
            if y > d.y + d.h - 24.0 {
                break;
            }
            f.text(&l, x, y, style::FONT_S, style::TEXT);
            y += 20.0;
        }
    }
}

impl MapList {
    pub fn buttons(&self, screen: P2) -> Vec<Button<MenuAction>> {
        let rows = self
            .entries
            .iter()
            .map(|e| (e.map.id.to_uppercase(), if e.generated { "ON DISK" } else { "NOT GENERATED" }.to_owned()))
            .collect();
        list_buttons(screen, rows, self.sel, "EDIT MAP", !self.entries.is_empty())
    }

    pub fn draw(&self, f: &mut Frame, bd: &Backdrop, screen: P2, pv: &Previews) {
        bd.draw(f, screen);
        heading(
            f,
            40.0,
            50.0,
            "TERRAIN EDITOR",
            "PICK A MAP; A MAP NOT YET ON DISK IS GENERATED FROM ITS RECIPE FIRST",
        );
        let l = list_frame(screen);
        panel(f, l);
        f.text(&format!("MAPS ({})", self.entries.len()), l.x + 12.0, l.y + 10.0, style::FONT, style::TEXT_DIM);
        let d = detail_frame(screen);
        panel(f, d);
        let Some(e) = self.entries.get(self.sel) else { return };
        let (x, mut y) = (d.x + 20.0, d.y + 18.0);
        f.text(&format!("MAP {}", e.map.id.to_uppercase()), x, y, style::FONT_L, style::AMBER);
        y += 44.0;
        let side = 300.0f32.min(d.h - 60.0).min(d.w * 0.4);
        pv.draw(f, &e.map.id, Rect::new(d.x + d.w - side - 20.0, d.y + 18.0, side, side));
        let tw = d.w - side - 220.0;
        let used = if e.used_by.is_empty() { "NO SCENARIO".to_owned() } else { e.used_by.join(", ").to_uppercase() };
        for (k, v) in [
            ("RECIPE", e.map.recipe.to_uppercase()),
            ("SEED", e.map.seed.to_string()),
            ("SIZE", format!("{:.0} × {:.0} KM", e.map.size_m / 1000.0, e.map.size_m / 1000.0)),
            ("STATUS", if e.generated { "AUTHORING PACKAGE ON DISK".into() } else { "GENERATED ON OPEN".into() }),
            ("USED BY", used),
        ] {
            f.text(k, x, y, style::FONT_S, style::TEXT_DIM);
            for (n, l) in wrap(f, &v, style::FONT_S, tw).iter().enumerate() {
                f.text(l, x + 120.0, y + n as f32 * 20.0, style::FONT_S, style::TEXT);
                if n > 0 {
                    y += 20.0;
                }
            }
            y += 22.0;
        }
        y += 14.0;
        for l in wrap(
            f,
            "EDITS CHANGE THE AUTHORING PACKAGE (DATA/MAPS/<ID>); SAVING ALSO WRITES THE BAKE. EVERY SCENARIO ON THIS MAP USES THE SAVED GROUND THE NEXT TIME IT STARTS.",
            style::FONT_S,
            d.w - 40.0 - if y < d.y + side + 30.0 { side + 20.0 } else { 0.0 },
        ) {
            f.text(&l, x, y, style::FONT_S, style::TEXT_DIM);
            y += 20.0;
        }
    }
}

// ---------------------------------------------------------------- loading

pub fn draw_loading(
    f: &mut Frame,
    bd: &Backdrop,
    screen: P2,
    what: &str,
    lines: &[String],
    error: Option<&str>,
    t: f32,
) {
    bd.draw(f, screen);
    let r = modal(screen, 640.0, 220.0);
    panel(f, r);
    let head = if error.is_some() { "LOADING FAILED" } else { "LOADING" };
    f.text(head, r.x + 20.0, r.y + 18.0, style::FONT_S, if error.is_some() { style::HOSTILE } else { style::TEXT_DIM });
    f.text(&short(what, 40), r.x + 20.0, r.y + 40.0, style::FONT_L, style::AMBER);
    let mut y = r.y + 82.0;
    if let Some(e) = error {
        for l in wrap(f, e, style::FONT_S, r.w - 40.0).iter().take(4) {
            f.text(l, r.x + 20.0, y, style::FONT_S, style::HOSTILE);
            y += 20.0;
        }
        f.text("ESC / ENTER: BACK TO THE MAIN MENU", r.x + 20.0, r.y + r.h - 30.0, style::FONT_S, style::TEXT_DIM);
        return;
    }
    let shown: Vec<&String> = lines.iter().rev().take(4).collect();
    for l in shown.into_iter().rev() {
        f.text(&short(&l.to_uppercase(), 70), r.x + 20.0, y, style::FONT_S, style::TEXT);
        y += 20.0;
    }
    // Progress ticker: a dot moving along the bottom edge (work time is unknown).
    let p = (t * 0.6).fract();
    let (x0, x1, by) = (r.x + 20.0, r.x + r.w - 20.0, r.y + r.h - 22.0);
    f.line([x0, by], [x1, by], 1.0, style::PANEL_EDGE);
    f.rect(x0 + (x1 - x0 - 40.0) * p, by - 1.0, 40.0, 3.0, style::AMBER);
}

// ---------------------------------------------------------------- overlays

pub fn options_buttons(screen: P2, s: &Settings) -> Vec<Button<MenuAction>> {
    let r = modal(screen, 600.0, 661.0);
    let on = |b: bool| if b { "ON" } else { "OFF" }.to_owned();
    let rows: [(&str, String, Opt, &str); 10] = [
        (
            "DISPLAY",
            if s.fullscreen { "FULLSCREEN".into() } else { "WINDOW".into() },
            Opt::Fullscreen,
            "BORDERLESS FULLSCREEN ON THE CURRENT MONITOR  (F11)",
        ),
        ("FRAME RATE", s.fps_label(), Opt::Fps, "FRAME-RATE CAP (VSYNC ALWAYS ON)"),
        (
            "UI FONT",
            if s.ttf_font { "IBM PLEX MONO".into() } else { "BUILT-IN".into() },
            Opt::Font,
            "TRUETYPE UI FONT OR RAYLIB'S BUILT-IN PIXEL FONT",
        ),
        ("PAN SPEED", s.pan_label().into(), Opt::Pan, "ARROW-KEY AND EDGE PAN SPEED"),
        ("EDGE PAN", on(s.edge_pan), Opt::EdgePan, "PAN THE MAP WHEN THE MOUSE TOUCHES A SCREEN EDGE"),
        ("CONTOURS", on(s.contours), Opt::Contours, "CONTOUR LINES WHEN A MAP OPENS (F5 TOGGLES)"),
        ("GRID", on(s.grid), Opt::Grid, "1 KM GRID WHEN A MAP OPENS (F6 TOGGLES)"),
        (
            "PAUSE ON MENU",
            on(s.pause_on_menu),
            Opt::PauseOnMenu,
            "THE CLOCK STOPS WHILE A CONTEXT MENU IS OPEN OR A PLAN IS EDITED",
        ),
        (
            "PAUSE WHEN SET",
            on(s.pause_on_ready),
            Opt::PauseOnReady,
            "THE CLOCK STOPS WHEN EVERY PLATOON WAITING ON A CODE WORD REPORTS SET",
        ),
        (
            "AUTO-PAUSE",
            s.auto_pause.label().into(),
            Opt::AutoPause,
            "THE CLOCK STOPS ON A NEW CONTACT, AND OPTIONALLY ON A VEHICLE LOST OR INDIRECT FIRE",
        ),
    ];
    let mut out: Vec<Button<MenuAction>> = rows
        .into_iter()
        .enumerate()
        .map(|(k, (l, v, o, tip))| {
            menu_button(
                Rect::new(r.x + 20.0, r.y + 70.0 + k as f32 * (ROW_H + ROW_GAP), r.w - 40.0, ROW_H),
                l,
                MenuAction::Opt(o),
                "",
            )
            .value(v)
            .tip(tip)
        })
        .collect();
    let y = r.y + r.h - 20.0 - ROW_H;
    out.push(menu_button(Rect::new(r.x + 20.0, y, 200.0, ROW_H), "DEFAULTS", MenuAction::Defaults, ""));
    out.push(menu_button(Rect::new(r.x + r.w - 220.0, y, 200.0, ROW_H), "BACK", MenuAction::Back, "ESC"));
    out
}

pub fn draw_options(f: &mut Frame, screen: P2, note: &str) {
    let r = modal(screen, 600.0, 661.0);
    f.rect(0.0, 0.0, screen[0], screen[1], Rgba(0, 0, 0, 120));
    panel(f, r);
    f.text("OPTIONS", r.x + 20.0, r.y + 20.0, style::FONT_L, style::AMBER);
    f.text("CLICK OR ENTER TO CHANGE; SAVED AT ONCE", r.x + 20.0, r.y + 46.0, style::FONT_S, style::TEXT_DIM);
    f.text(&short(note, 70), r.x + 20.0, r.y + r.h - 80.0, style::FONT, style::TEXT_OFF);
}

pub fn pause_buttons(screen: P2, items: &[(&'static str, ScreenAction, &'static str)]) -> Vec<Button<MenuAction>> {
    let n = items.len() + 6;
    let h = 70.0 + n as f32 * (ROW_H + ROW_GAP) + 10.0;
    let r = modal(screen, 440.0, h);
    let mut v: Vec<(String, MenuAction, &'static str)> = vec![("RESUME".into(), MenuAction::Resume, "ESC")];
    v.extend(items.iter().map(|(l, a, k)| ((*l).to_owned(), MenuAction::Screen(*a), *k)));
    v.extend([
        ("DATABASE".to_owned(), MenuAction::Database, ""),
        ("OPTIONS".to_owned(), MenuAction::Options, ""),
        ("HELP".to_owned(), MenuAction::Help, "F7"),
        ("MAIN MENU".to_owned(), MenuAction::MainMenu, ""),
        ("QUIT TO DESKTOP".to_owned(), MenuAction::Quit, ""),
    ]);
    column(r.x + 20.0, r.y + 64.0, r.w - 40.0, v)
}

pub fn draw_pause(f: &mut Frame, screen: P2, title: &str, n_items: usize) {
    let n = n_items + 6;
    let h = 70.0 + n as f32 * (ROW_H + ROW_GAP) + 10.0;
    let r = modal(screen, 440.0, h);
    f.rect(0.0, 0.0, screen[0], screen[1], Rgba(0, 0, 0, 120));
    panel(f, r);
    f.text("MENU", r.x + 20.0, r.y + 16.0, style::FONT, style::TEXT_DIM);
    f.text(&short(title, 34), r.x + 20.0, r.y + 32.0, style::FONT_S, style::AMBER);
}

pub fn confirm_buttons(screen: P2) -> Vec<Button<MenuAction>> {
    let r = modal(screen, 520.0, 170.0);
    let y = r.y + r.h - 20.0 - ROW_H;
    vec![
        menu_button(Rect::new(r.x + 20.0, y, 230.0, ROW_H), "CANCEL", MenuAction::Back, "ESC"),
        menu_button(Rect::new(r.x + r.w - 250.0, y, 230.0, ROW_H), "DISCARD", MenuAction::Discard, ""),
    ]
}

pub fn draw_confirm(f: &mut Frame, screen: P2) {
    let r = modal(screen, 520.0, 170.0);
    f.rect(0.0, 0.0, screen[0], screen[1], Rgba(0, 0, 0, 140));
    f.rect(r.x, r.y, r.w, r.h, style::PANEL);
    f.rect_lines(r.x, r.y, r.w, r.h, style::AMBER);
    f.text("UNSAVED CHANGES", r.x + 20.0, r.y + 20.0, style::FONT_L, style::AMBER);
    f.text("LEAVING NOW DISCARDS THEM. SAVE FROM THE MENU FIRST.", r.x + 20.0, r.y + 52.0, style::FONT_S, style::TEXT);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class(id: &str) -> Option<VehicleClass> {
        match id {
            "mbt" => Some(VehicleClass::Mbt),
            "ifv" => Some(VehicleClass::Ifv),
            "scout" => Some(VehicleClass::Recon),
            _ => None,
        }
    }

    #[test]
    fn version_label_marks_untagged_and_dev_builds() {
        assert_eq!(version_label("0.1.0", Some("v0.1.0")), "0.1.0");
        assert_eq!(version_label("0.1.0", Some("v0.1.0-3-g1a2b3c4")), "0.1.0-3-G1A2B3C4");
        assert_eq!(version_label("0.1.0", Some("1a2b3c4-dirty")), "0.1.0+1A2B3C4-DIRTY");
        assert_eq!(version_label("0.1.0", Some("")), "0.1.0 DEV");
        assert_eq!(version_label("0.1.0", None), "0.1.0 DEV");
    }

    #[test]
    fn force_summary_counts_by_class_not_all_as_tanks() {
        let blue = ["mbt"; 8].into_iter().chain(["ifv"; 3]);
        assert_eq!(force_summary(blue, class), "8 TANKS · 3 IFVS");
        assert_eq!(force_summary(["mbt", "ifv", "scout", "scout"], class), "1 TANK · 1 IFV · 2 RECON");
    }

    #[test]
    fn force_summary_handles_empty_and_unknown_vehicles() {
        assert_eq!(force_summary([], class), "NO VEHICLES");
        assert_eq!(force_summary(["mbt", "ghost"], class), "1 TANK · 1 UNKNOWN");
    }
}
