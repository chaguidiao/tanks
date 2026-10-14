//! Tanks client: title, main menu, scenario/map select, game, terrain editor, scenario editor.
//!
//! ```text
//! client                                   title screen → main menu
//! client [--screen title|main|play|edit-scenario|edit-terrain|database|options]
//! client [--scenario <id>] [--map <id>] [--edit] [--scenario-editor] [--pause]
//!        [--screenshot <png> [--frames <n>] [--order <unit-or-platoon>:<x>,<y>[,<x>,<y>..]] [--bp <platoon>:<x>,<y>,<deg>]
//!         [--pl -:<x>,<y>,<x>,<y>..] [--plan <platoon>:<x>,<y>,..|@<plan.ron> [--plan-draft]]
//!         [--advance <s>] [--code-word <name>] [--advance <s>].. [--aar] [--save-replay <ron>]
//!         [--zoom <m-per-px>] [--center <x>,<y>] [--layer relief|slope|traffic|cover]]
//! ```
//! Any game or editor option starts that screen directly (scripted checks); `--pause` opens
//! its menu. `--keys enter,wait30,down,esc` injects one key per frame (menu-flow checks).
//! `--advance` and `--code-word` run in the order given (a plan released by a code word).
//! Screenshot runs use default options and never write the options file.

mod aar;
mod database;
mod editor;
mod format;
mod game;
mod hud;
mod mapscreen;
mod menu;
mod render;
mod scenedit;
mod settings;
mod style;
mod symbols;
mod view;

use std::path::PathBuf;
use std::sync::{Arc, mpsc};

use sim_data::{DataSet, MapRef, ScenarioDef};
use terragen::workflow;
use terrain::{AuthoringPackage, BakedMap};

use crate::game::{Cmd, Game};
use crate::mapscreen::{Ctx, MapScreen, Request};
use crate::menu::{Backdrop, MapEntry, MapList, MenuAction, Opt, Purpose, ScenarioEntry, ScenarioList};
use crate::render::{Gfx, Input, Key};
use crate::settings::Settings;
use crate::view::BaseLayer;

struct Opts {
    scenario: Option<String>,
    map: Option<String>,
    edit: bool,
    scenario_editor: bool,
    screen: Option<String>,
    pause: bool,
    aar: bool,
    save_replay: Option<String>,
    screenshot: Option<String>,
    frames: u32,
    /// ("--order" | "--bp", callsign, numbers)
    orders: Vec<(String, String, Vec<f64>)>,
    /// `--plan <platoon>:<x>,<y>,..` or `--plan <platoon>:@<file.ron>` (a `Plan` in RON).
    plans: Vec<(String, String)>,
    /// Leave `--plan` as a draft on the map instead of sending it.
    plan_draft: bool,
    /// `--advance <s>` and `--code-word <name>`, in argument order.
    timeline: Vec<Step>,
    zoom: Option<f64>,
    center: Option<[f64; 2]>,
    layer: Option<String>,
    select: Option<String>,
    press: Option<String>,
    keys: Option<String>,
    /// `--rclick x,y[,hold_s]`: right click at a screen position (held = context menu).
    rclick: Option<([f32; 2], f32)>,
}

impl Opts {
    /// A game or editor option was given: skip the menus.
    fn direct(&self) -> bool {
        self.scenario.is_some()
            || self.map.is_some()
            || self.edit
            || self.scenario_editor
            || self.aar
            || self.pause
            || self.save_replay.is_some()
            || !self.orders.is_empty()
            || !self.timeline.is_empty()
            || !self.plans.is_empty()
            || self.select.is_some()
            || self.press.is_some()
            || self.zoom.is_some()
            || self.center.is_some()
            || self.layer.is_some()
            || self.rclick.is_some()
    }
}

enum Step {
    Advance(f64),
    CodeWord(String),
}

fn parse_xy(s: &str) -> Option<[f64; 2]> {
    let (x, y) = s.split_once(',')?;
    Some([x.trim().parse().ok()?, y.trim().parse().ok()?])
}

fn opts() -> Result<Opts, String> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let get = |k: &str| a.iter().position(|x| x == k).and_then(|i| a.get(i + 1)).cloned();
    let mut orders = Vec::new();
    let mut plans = Vec::new();
    let mut timeline = Vec::new();
    for (i, x) in a.iter().enumerate() {
        let next = a.get(i + 1);
        match (x.as_str(), next) {
            ("--plan", Some(v)) => {
                let (cs, rest) = v.split_once(':').ok_or("--plan needs <platoon>:<points or @file>")?;
                plans.push((cs.to_owned(), rest.to_owned()));
            }
            ("--advance", Some(v)) => timeline.push(Step::Advance(v.parse().map_err(|_| "bad --advance")?)),
            ("--code-word", Some(v)) => timeline.push(Step::CodeWord(v.to_uppercase())),
            _ => {}
        }
        if x == "--order" || x == "--bp" || x == "--pl" {
            let v = a.get(i + 1).ok_or("--order/--bp needs <callsign>:<numbers>")?;
            let (cs, nums) = v.split_once(':').ok_or("--order/--bp needs <callsign>:<numbers>")?;
            let nums: Vec<f64> =
                nums.split(',').map(|n| n.trim().parse::<f64>().map_err(|_| "bad number")).collect::<Result<_, _>>()?;
            orders.push((x.clone(), cs.to_owned(), nums));
        }
    }
    Ok(Opts {
        scenario: get("--scenario"),
        map: get("--map"),
        edit: a.iter().any(|x| x == "--edit"),
        scenario_editor: a.iter().any(|x| x == "--scenario-editor"),
        screen: get("--screen"),
        pause: a.iter().any(|x| x == "--pause"),
        aar: a.iter().any(|x| x == "--aar"),
        save_replay: get("--save-replay"),
        screenshot: get("--screenshot"),
        frames: get("--frames").and_then(|v| v.parse().ok()).unwrap_or(6),
        orders,
        plans,
        plan_draft: a.iter().any(|x| x == "--plan-draft"),
        timeline,
        zoom: get("--zoom").and_then(|v| v.parse().ok()),
        center: get("--center").and_then(|v| parse_xy(&v)),
        layer: get("--layer"),
        select: get("--select"),
        press: get("--press"),
        keys: get("--keys"),
        rclick: match get("--rclick") {
            None => None,
            Some(v) => {
                let n: Vec<f32> =
                    v.split(',').map(|x| x.trim().parse().map_err(|_| "bad --rclick")).collect::<Result<_, _>>()?;
                match n[..] {
                    [x, y] => Some(([x, y], 0.0)),
                    [x, y, h] => Some(([x, y], h)),
                    _ => return Err("--rclick needs x,y[,hold_s]".into()),
                }
            }
        },
    })
}

/// A map reference for a map directory no scenario names (the map exists, so it only loads).
fn loose_map(id: &str) -> MapRef {
    MapRef { id: id.to_owned(), recipe: "rolling-farmland".into(), seed: 1, size_m: 8000.0 }
}

enum Screen {
    Title,
    Main,
    Scenarios(ScenarioList),
    Maps(MapList),
    Loading(Box<Loading>),
    Map(Box<MapScreen>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Leave {
    MainMenu,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Overlay {
    /// Screen menu of a map screen (Esc).
    Pause,
    /// Options; `over_pause` = BACK returns to the screen menu.
    Options { over_pause: bool },
    /// Unsaved edits would be lost.
    Confirm(Leave),
}

/// What a finished load opens.
enum Target {
    Game(ScenarioDef),
    ScenarioEditor(ScenarioDef),
    TerrainEditor,
}

type LoadResult = Result<(BakedMap, Option<AuthoringPackage>), String>;

enum LoadMsg {
    Log(String),
    Done(Box<LoadResult>),
}

/// A map being generated / baked / loaded on a worker thread (the window keeps drawing).
struct Loading {
    what: String,
    map_id: String,
    lines: Vec<String>,
    rx: mpsc::Receiver<LoadMsg>,
    target: Option<Target>,
    error: Option<String>,
}

struct App {
    // Screens hold textures: declared before `gfx` so they are dropped before the window.
    screen: Screen,
    /// A game left for the main menu (RESUME).
    suspended: Option<Box<MapScreen>>,
    overlay: Option<Overlay>,
    /// DATABASE screen, over the main menu or a map screen (which stays paused under it).
    database: Option<database::Database>,
    help: bool,
    focus: Option<usize>,
    backdrop: Backdrop,
    settings: Settings,
    /// Write option changes to disk (not in screenshot runs).
    persist: bool,
    status: String,
    data_dir: PathBuf,
    ds: Arc<DataSet>,
    font_path: PathBuf,
    t: f32,
    quit: bool,
    last_input: Input,
    /// `--keys`: injected key presses (None = an idle frame).
    script_keys: std::collections::VecDeque<Option<Key>>,
    /// `--rclick`: (screen position, seconds held, frames run).
    script_rclick: Option<([f32; 2], f32, u32)>,
    script_mouse: Option<[f32; 2]>,
    gfx: Gfx,
}

/// `--keys` tokens: key names, single letters/digits, `waitN` = N idle frames.
fn parse_keys(s: &str) -> Result<std::collections::VecDeque<Option<Key>>, String> {
    let mut out = std::collections::VecDeque::new();
    for t in s.split(',').map(str::trim).filter(|t| !t.is_empty()) {
        let k = match t.to_ascii_lowercase().as_str() {
            "enter" => Key::Enter,
            "esc" => Key::Escape,
            "up" => Key::Up,
            "down" => Key::Down,
            "left" => Key::Left,
            "right" => Key::Right,
            "space" => Key::Space,
            "tab" => Key::Tab,
            w if w.starts_with("wait") => {
                let n: usize = w[4..].parse().map_err(|_| format!("bad --keys token {t}"))?;
                out.extend(std::iter::repeat_n(None, n));
                continue;
            }
            f if f.len() > 1 && f.starts_with('f') => Key::F(f[1..].parse().map_err(|_| format!("bad key {t}"))?),
            c if c.len() == 1 => {
                let c = c.chars().next().expect("one char");
                match c.to_digit(10) {
                    Some(d) => Key::Num(d as u8),
                    None => Key::Char(c),
                }
            }
            _ => return Err(format!("bad --keys token {t}")),
        };
        out.push_back(Some(k));
    }
    Ok(out)
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let o = opts()?;
    let data_dir = sim_data::find_data_dir().ok_or("cannot find data/ directory (set TANKS_DATA)")?;
    let ds = Arc::new(DataSet::load(&data_dir).map_err(|e| e.to_string())?);
    let shot = o.screenshot.is_some();
    let settings = if shot { Settings::default() } else { Settings::load() };

    let mut gfx = Gfx::open(1600, 900, "TANKS", shot);
    let font_path = data_dir.join("fonts/IBMPlexMono-Medium.ttf");
    if settings.ttf_font
        && let Err(e) = gfx.load_font(&font_path)
    {
        eprintln!("warning: UI font not loaded, using the built-in font: {e}");
    }
    if !shot {
        gfx.set_fps(settings.fps_cap);
        gfx.set_fullscreen(settings.fullscreen);
    }

    let mut app = App {
        screen: Screen::Title,
        suspended: None,
        overlay: None,
        database: None,
        help: false,
        focus: None,
        backdrop: Backdrop::new(),
        settings,
        persist: !shot,
        status: String::new(),
        data_dir,
        ds,
        font_path,
        t: 0.0,
        quit: false,
        last_input: Input::default(),
        script_keys: o.keys.as_deref().map(parse_keys).transpose()?.unwrap_or_default(),
        script_rclick: o.rclick.map(|(p, h)| (p, h, 0)),
        script_mouse: None,
        gfx,
    };
    if o.direct() {
        app.screen = Screen::Map(Box::new(app.direct(&o)?));
        if o.pause {
            app.overlay = Some(Overlay::Pause);
        }
    } else if let Some(s) = &o.screen {
        match s.as_str() {
            "title" => {}
            "main" => app.screen = Screen::Main,
            "play" => app.screen = Screen::Scenarios(app.scenario_list(Purpose::Play)),
            "edit-scenario" => app.screen = Screen::Scenarios(app.scenario_list(Purpose::Edit)),
            "edit-terrain" => app.screen = Screen::Maps(app.map_list()),
            "database" => {
                app.screen = Screen::Main;
                app.database = Some(database::Database::new(false));
            }
            "options" => {
                app.screen = Screen::Main;
                app.overlay = Some(Overlay::Options { over_pause: false });
            }
            _ => return Err(format!("unknown --screen {s}")),
        }
    }

    let mut frame = 0u32;
    while !app.gfx.should_close() && !app.quit {
        app.tick()?;
        // Loads take wall-clock time: scripted runs count only frames outside them.
        if !matches!(app.screen, Screen::Loading(_)) {
            frame += 1;
        }
        if let Some(path) = &o.screenshot
            && frame >= o.frames
        {
            app.gfx.screenshot(path);
            break;
        }
    }
    Ok(())
}

impl App {
    fn ctx(&self) -> Ctx<'_> {
        Ctx { ds: &self.ds, data_dir: &self.data_dir, settings: &self.settings }
    }

    /// Scripted start (CLI options): load synchronously, apply orders, advance.
    fn direct(&mut self, o: &Opts) -> Result<MapScreen, String> {
        let load = |id: &str| DataSet::load_scenario(&self.data_dir, id).map_err(|e| e.to_string());
        let scenario = match (&o.scenario, &o.map) {
            (Some(s), _) => Some(load(s)?),
            (None, Some(_)) => None,
            (None, None) => Some(load("m1-move")?),
        };
        let map_ref = match (&scenario, &o.map) {
            (Some(s), _) => s.map.clone(),
            (None, Some(id)) => loose_map(id),
            _ => unreachable!(),
        };
        let map = Arc::new(
            workflow::ensure_map(&self.data_dir, &self.ds, &map_ref, |s| eprintln!("{s}"))
                .map_err(|e| e.to_string())?,
        );
        let ctx = self.ctx();
        let mut ms = match scenario {
            Some(s) if o.scenario_editor && !o.edit => MapScreen::scenario_editor(map, s, &ctx),
            Some(s) if !o.edit => MapScreen::game(map, s, &ctx)?,
            _ => {
                let pkg = AuthoringPackage::load(&workflow::map_dir(&self.data_dir, &map_ref.id))
                    .map_err(|e| e.to_string())?;
                MapScreen::terrain_editor(map_ref.id.clone(), map, pkg, &ctx)
            }
        };
        if let Some(l) = &o.layer {
            ms.view.base = match l.as_str() {
                "slope" => BaseLayer::Slope,
                "traffic" => BaseLayer::Trafficability,
                "cover" => BaseLayer::LandCover,
                _ => BaseLayer::Tactical,
            };
        }
        if let Some(z) = o.zoom {
            ms.cam.mpp = z;
        }
        if let Some(c) = o.center {
            ms.cam.centre = c;
        }
        if let Some(g) = ms.game_mut() {
            script(g, o)?;
        }
        Ok(ms)
    }

    fn scenario_list(&self, purpose: Purpose) -> ScenarioList {
        let entries = DataSet::scenario_ids(&self.data_dir)
            .into_iter()
            .map(|id| {
                let def = DataSet::load_scenario(&self.data_dir, &id).map_err(|e| e.to_string());
                ScenarioEntry { id, def }
            })
            .collect();
        ScenarioList { purpose, entries, sel: 0 }
    }

    /// Maps named by scenarios, then map directories no scenario names.
    fn map_list(&self) -> MapList {
        let mut entries: Vec<MapEntry> = Vec::new();
        for id in DataSet::scenario_ids(&self.data_dir) {
            let Ok(def) = DataSet::load_scenario(&self.data_dir, &id) else { continue };
            match entries.iter_mut().find(|e| e.map.id == def.map.id) {
                Some(e) => e.used_by.push(def.name.clone()),
                None => {
                    entries.push(MapEntry { map: def.map.clone(), generated: false, used_by: vec![def.name.clone()] })
                }
            }
        }
        let mut loose: Vec<String> = std::fs::read_dir(self.data_dir.join("maps"))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().join("map.toml").exists())
            .filter_map(|e| e.file_name().to_str().map(str::to_owned))
            .filter(|id| !entries.iter().any(|e| &e.map.id == id))
            .collect();
        loose.sort();
        entries.extend(loose.iter().map(|id| MapEntry { map: loose_map(id), generated: true, used_by: Vec::new() }));
        for e in &mut entries {
            e.generated = workflow::map_dir(&self.data_dir, &e.map.id).join("map.toml").exists();
        }
        MapList { entries, sel: 0 }
    }

    fn start_load(&mut self, what: String, map: MapRef, target: Target) {
        let (tx, rx) = mpsc::channel();
        let (dir, ds) = (self.data_dir.clone(), self.ds.clone());
        let want_pkg = matches!(target, Target::TerrainEditor);
        let map_id = map.id.clone();
        std::thread::spawn(move || {
            let log = tx.clone();
            let r = workflow::ensure_map(&dir, &ds, &map, |s| {
                let _ = log.send(LoadMsg::Log(s.to_owned()));
            })
            .map_err(|e| e.to_string())
            .and_then(|b| {
                if !want_pkg {
                    return Ok((b, None));
                }
                let pkg = AuthoringPackage::load(&workflow::map_dir(&dir, &map.id)).map_err(|e| e.to_string())?;
                Ok((b, Some(pkg)))
            });
            let _ = tx.send(LoadMsg::Done(Box::new(r)));
        });
        self.go(Screen::Loading(Box::new(Loading {
            what,
            map_id,
            lines: vec!["LOADING MAP".into()],
            rx,
            target: Some(target),
            error: None,
        })));
    }

    fn go(&mut self, s: Screen) {
        self.screen = s;
        self.focus = None;
        self.help = false;
    }

    fn apply_option(&mut self, o: Opt) {
        let s = &mut self.settings;
        match o {
            Opt::Fullscreen => s.fullscreen = !s.fullscreen,
            Opt::Fps => s.next_fps(),
            Opt::Font => s.ttf_font = !s.ttf_font,
            Opt::Pan => s.next_pan(),
            Opt::EdgePan => s.edge_pan = !s.edge_pan,
            Opt::Contours => s.contours = !s.contours,
            Opt::Grid => s.grid = !s.grid,
            Opt::RightClick => s.right_click_menu = !s.right_click_menu,
            Opt::PauseOnMenu => s.pause_on_menu = !s.pause_on_menu,
            Opt::PauseOnReady => s.pause_on_ready = !s.pause_on_ready,
        }
        self.settings_changed();
    }

    fn settings_changed(&mut self) {
        self.gfx.set_fullscreen(self.settings.fullscreen);
        self.gfx.set_fps(self.settings.fps_cap);
        if self.settings.ttf_font {
            if let Err(e) = self.gfx.load_font(&self.font_path) {
                self.status = format!("FONT NOT LOADED: {e}");
                self.settings.ttf_font = false;
            }
        } else {
            self.gfx.builtin_font();
        }
        if self.persist
            && let Err(e) = self.settings.save()
        {
            self.status = format!("OPTIONS NOT SAVED: {e}");
        }
    }

    /// Leave the current map screen (after any unsaved-changes prompt).
    fn leave(&mut self, to: Leave) {
        self.overlay = None;
        let s = std::mem::replace(&mut self.screen, Screen::Main);
        if let (Screen::Map(ms), Leave::MainMenu) = (s, to)
            && matches!(ms.mode, mapscreen::Mode::Game { .. })
        {
            self.suspended = Some(ms);
        }
        self.go(Screen::Main);
        if to == Leave::Quit {
            self.quit = true;
        }
    }

    fn request_leave(&mut self, to: Leave) {
        let unsaved = matches!(&self.screen, Screen::Map(ms) if ms.unsaved());
        if unsaved {
            self.overlay = Some(Overlay::Confirm(to));
            self.focus = None;
        } else {
            self.leave(to);
        }
    }

    fn tick(&mut self) -> Result<(), String> {
        let mut inp = self.gfx.input();
        if let Some((pos, hold, frame)) = &mut self.script_rclick
            && matches!(self.screen, Screen::Map(_))
        {
            *frame += 1;
            let down_frames = (*hold / 0.1).ceil() as u32 + 1;
            let f = *frame;
            self.script_mouse = Some(*pos);
            inp.frame_time = 0.1;
            inp.right_pressed = f == 3;
            inp.right_down = (3..3 + down_frames).contains(&f);
            inp.right_released = f == 3 + down_frames;
            if f > 3 + down_frames {
                self.script_rclick = None;
            }
        }
        if let Some(m) = self.script_mouse {
            inp.mouse = m;
        }
        if !matches!(self.screen, Screen::Loading(_))
            && self.script_rclick.is_none()
            && let Some(Some(k)) = self.script_keys.pop_front()
        {
            inp.pressed.push(k);
        }
        self.t += inp.frame_time;
        if inp.hit(Key::F(11)) {
            self.apply_option(Opt::Fullscreen);
        }
        if self.help {
            if inp.hit(Key::F(7)) || inp.hit(Key::Escape) || inp.left_pressed {
                self.help = false;
            }
        } else if let Some(db) = &mut self.database {
            if inp.hit(Key::F(7)) {
                self.help = true;
            } else if db.update(&self.ds, &inp) {
                if db.from_pause {
                    self.overlay = Some(Overlay::Pause);
                }
                self.database = None;
                self.focus = None;
            }
        } else if let Some(o) = self.overlay {
            self.overlay_input(o, &inp);
        } else {
            self.screen_input(&inp);
        }
        if let Screen::Map(ms) = &mut self.screen {
            ms.ensure_view(&mut self.gfx, &self.ds);
        }
        self.last_input = inp;
        self.draw();
        Ok(())
    }

    fn overlay_input(&mut self, o: Overlay, inp: &Input) {
        let screen = inp.screen;
        let mut b = match o {
            Overlay::Pause => match &self.screen {
                Screen::Map(ms) => menu::pause_buttons(screen, &ms.menu_items()),
                _ => Vec::new(),
            },
            Overlay::Options { .. } => menu::options_buttons(screen, &self.settings),
            Overlay::Confirm(_) => menu::confirm_buttons(screen),
        };
        let key = menu::nav(&mut b, &mut self.focus, inp);
        let a = key.or_else(|| hud::clicked(&b, inp.mouse, inp.left_pressed));
        let back = inp.hit(Key::Escape) || inp.hit(Key::F(10));
        match (o, a) {
            (Overlay::Pause, Some(MenuAction::Resume)) | (Overlay::Pause, None) if back || a.is_some() => {
                self.overlay = None;
            }
            (Overlay::Pause, Some(MenuAction::Screen(sa))) => {
                self.overlay = None;
                self.run_screen_action(sa);
            }
            (Overlay::Pause, Some(MenuAction::Options)) => {
                self.overlay = Some(Overlay::Options { over_pause: true });
                self.focus = None;
            }
            (Overlay::Pause, Some(MenuAction::Database)) => {
                self.overlay = None;
                self.database = Some(database::Database::new(true));
            }
            (Overlay::Pause, Some(MenuAction::Help)) => {
                self.overlay = None;
                self.help = true;
            }
            (Overlay::Pause, Some(MenuAction::MainMenu)) => self.request_leave(Leave::MainMenu),
            (Overlay::Pause, Some(MenuAction::Quit)) => self.request_leave(Leave::Quit),
            (Overlay::Options { .. }, Some(MenuAction::Opt(opt))) => self.apply_option(opt),
            (Overlay::Options { .. }, Some(MenuAction::Defaults)) => {
                self.settings = Settings::default();
                self.settings_changed();
            }
            (Overlay::Options { over_pause }, Some(MenuAction::Back)) | (Overlay::Options { over_pause }, None)
                if back || a.is_some() =>
            {
                self.overlay = over_pause.then_some(Overlay::Pause);
                self.focus = None;
            }
            (Overlay::Confirm(to), Some(MenuAction::Discard)) => self.leave(to),
            (Overlay::Confirm(_), Some(MenuAction::Back)) | (Overlay::Confirm(_), None) if back || a.is_some() => {
                self.overlay = Some(Overlay::Pause);
                self.focus = None;
            }
            _ => {}
        }
    }

    fn run_screen_action(&mut self, sa: mapscreen::ScreenAction) {
        let Screen::Map(ms) = &mut self.screen else { return };
        let ctx = Ctx { ds: &self.ds, data_dir: &self.data_dir, settings: &self.settings };
        if let Some(r) = ms.act(sa, &ctx) {
            self.handle_request(r);
        }
    }

    fn handle_request(&mut self, r: Request) {
        match r {
            Request::Menu => {
                self.overlay = Some(Overlay::Pause);
                self.focus = None;
            }
            Request::Help => self.help = true,
            Request::VehicleData(id) => self.database = Some(database::Database::vehicle(&self.ds, &id, false)),
            Request::MainMenu => self.request_leave(Leave::MainMenu),
            Request::PlayTest | Request::BackToEditor => {
                let Screen::Map(ms) = std::mem::replace(&mut self.screen, Screen::Main) else { return };
                let ctx = Ctx { ds: &self.ds, data_dir: &self.data_dir, settings: &self.settings };
                let ms = if r == Request::PlayTest { ms.play_test(&ctx) } else { ms.back_to_editor() };
                self.screen = Screen::Map(Box::new(ms));
            }
        }
    }

    fn screen_input(&mut self, inp: &Input) {
        let screen = inp.screen;
        let back = inp.hit(Key::Escape);
        match &mut self.screen {
            Screen::Title => {
                if !inp.pressed.is_empty() || inp.left_pressed || inp.right_pressed {
                    self.go(Screen::Main);
                }
            }
            Screen::Main => {
                let title = self.suspended.as_ref().map(|s| s.title());
                let mut b = menu::main_buttons(screen, title.as_deref());
                let a =
                    menu::nav(&mut b, &mut self.focus, inp).or_else(|| hud::clicked(&b, inp.mouse, inp.left_pressed));
                match a {
                    Some(MenuAction::Resume) => {
                        if let Some(s) = self.suspended.take() {
                            self.go(Screen::Map(s));
                        }
                    }
                    Some(MenuAction::Play) => self.go(Screen::Scenarios(self.scenario_list(Purpose::Play))),
                    Some(MenuAction::EditScenario) => self.go(Screen::Scenarios(self.scenario_list(Purpose::Edit))),
                    Some(MenuAction::EditTerrain) => self.go(Screen::Maps(self.map_list())),
                    Some(MenuAction::Database) => self.database = Some(database::Database::new(false)),
                    Some(MenuAction::Options) => {
                        self.overlay = Some(Overlay::Options { over_pause: false });
                        self.focus = None;
                    }
                    Some(MenuAction::Quit) => self.quit = true,
                    _ => {}
                }
            }
            Screen::Scenarios(l) => {
                menu::list_keys(inp, &mut l.sel, l.entries.len());
                let b = l.buttons(screen);
                let a = hud::clicked(&b, inp.mouse, inp.left_pressed)
                    .or(inp.hit(Key::Enter).then_some(MenuAction::Start))
                    .or(back.then_some(MenuAction::Back));
                match a {
                    Some(MenuAction::Pick(k)) => l.sel = k,
                    Some(MenuAction::Back) => self.go(Screen::Main),
                    Some(MenuAction::Start) => {
                        if let Some(Ok(def)) = l.entries.get(l.sel).map(|e| &e.def) {
                            let (def, purpose) = (def.clone(), l.purpose);
                            let what = def.name.to_uppercase();
                            let map = def.map.clone();
                            let target =
                                if purpose == Purpose::Play { Target::Game(def) } else { Target::ScenarioEditor(def) };
                            if purpose == Purpose::Play {
                                self.suspended = None;
                            }
                            self.start_load(what, map, target);
                        }
                    }
                    _ => {}
                }
            }
            Screen::Maps(l) => {
                menu::list_keys(inp, &mut l.sel, l.entries.len());
                let b = l.buttons(screen);
                let a = hud::clicked(&b, inp.mouse, inp.left_pressed)
                    .or(inp.hit(Key::Enter).then_some(MenuAction::Start))
                    .or(back.then_some(MenuAction::Back));
                match a {
                    Some(MenuAction::Pick(k)) => l.sel = k,
                    Some(MenuAction::Back) => self.go(Screen::Main),
                    Some(MenuAction::Start) => {
                        if let Some(e) = l.entries.get(l.sel) {
                            let map = e.map.clone();
                            self.start_load(format!("MAP {}", map.id.to_uppercase()), map, Target::TerrainEditor);
                        }
                    }
                    _ => {}
                }
            }
            Screen::Loading(ld) => {
                if ld.error.is_some() {
                    if back || inp.hit(Key::Enter) || inp.left_pressed {
                        self.go(Screen::Main);
                    }
                    return;
                }
                let mut done = None;
                loop {
                    match ld.rx.try_recv() {
                        Ok(LoadMsg::Log(s)) => ld.lines.push(s),
                        Ok(LoadMsg::Done(r)) => {
                            done = Some(*r);
                            break;
                        }
                        Err(mpsc::TryRecvError::Empty) => break,
                        Err(mpsc::TryRecvError::Disconnected) => {
                            done = Some(Err("loader thread stopped".into()));
                            break;
                        }
                    }
                }
                let Some(r) = done else { return };
                let target = ld.target.take().expect("target until done");
                let map_id = ld.map_id.clone();
                let ctx = Ctx { ds: &self.ds, data_dir: &self.data_dir, settings: &self.settings };
                let opened = r.and_then(|(map, pkg)| {
                    let map = Arc::new(map);
                    Ok(match (target, pkg) {
                        (Target::Game(def), _) => MapScreen::game(map, def, &ctx)?,
                        (Target::ScenarioEditor(def), _) => MapScreen::scenario_editor(map, def, &ctx),
                        (Target::TerrainEditor, Some(pkg)) => MapScreen::terrain_editor(map_id, map, pkg, &ctx),
                        (Target::TerrainEditor, None) => return Err("no authoring package".into()),
                    })
                });
                match opened {
                    Ok(ms) => self.go(Screen::Map(Box::new(ms))),
                    Err(e) => ld.error = Some(e.to_uppercase()),
                }
            }
            Screen::Map(ms) => {
                let ctx = Ctx { ds: &self.ds, data_dir: &self.data_dir, settings: &self.settings };
                if let Some(r) = ms.update(inp, &mut self.gfx, &ctx) {
                    self.handle_request(r);
                }
            }
        }
    }

    fn draw(&mut self) {
        let inp = &self.last_input;
        let (screen, mouse) = (inp.screen, inp.mouse);
        let (bd, t, ds) = (&self.backdrop, self.t, &self.ds);
        let (sc, overlay, help, settings, status) =
            (&self.screen, self.overlay, self.help, &self.settings, &self.status);
        let suspended = self.suspended.as_ref().map(|s| s.title());
        let focus = self.focus;
        let db = self.database.as_ref();
        let focused = |mut b: Vec<hud::Button<MenuAction>>| {
            for (k, x) in b.iter_mut().enumerate() {
                x.focused = Some(k) == focus;
            }
            b
        };
        self.gfx.frame(|f| {
            let mut help_lines: &[&str] = MENU_HELP;
            match sc {
                Screen::Title => menu::draw_title(f, bd, screen, t),
                Screen::Main if db.is_some() => {}
                Screen::Main => {
                    menu::draw_main(f, bd, screen, status);
                    if overlay.is_none() && db.is_none() {
                        menu::draw_buttons(
                            f,
                            &focused(menu::main_buttons(screen, suspended.as_deref())),
                            mouse,
                            screen,
                        );
                    }
                }
                Screen::Scenarios(l) => {
                    l.draw(f, bd, screen);
                    menu::draw_buttons(f, &l.buttons(screen), mouse, screen);
                }
                Screen::Maps(l) => {
                    l.draw(f, bd, screen);
                    menu::draw_buttons(f, &l.buttons(screen), mouse, screen);
                }
                Screen::Loading(ld) => {
                    menu::draw_loading(f, bd, screen, &ld.what, &ld.lines, ld.error.as_deref(), t);
                }
                Screen::Map(ms) => {
                    ms.draw(f, inp, ds);
                    help_lines = ms.help_lines();
                }
            }
            match overlay {
                Some(Overlay::Pause) => {
                    if let Screen::Map(ms) = sc {
                        let items = ms.menu_items();
                        menu::draw_pause(f, screen, &ms.title(), items.len());
                        menu::draw_buttons(f, &focused(menu::pause_buttons(screen, &items)), mouse, screen);
                    }
                }
                Some(Overlay::Options { .. }) => {
                    let note = Settings::path().map_or("OPTIONS ARE NOT SAVED (NO CONFIG DIRECTORY)".into(), |p| {
                        format!("FILE {}", p.display())
                    });
                    menu::draw_options(f, screen, &note);
                    menu::draw_buttons(f, &focused(menu::options_buttons(screen, settings)), mouse, screen);
                }
                Some(Overlay::Confirm(_)) => {
                    menu::draw_confirm(f, screen);
                    menu::draw_buttons(f, &focused(menu::confirm_buttons(screen)), mouse, screen);
                }
                None => {}
            }
            if let Some(db) = db {
                db.draw(f, bd, ds, inp, matches!(sc, Screen::Map(_)));
                help_lines = DATABASE_HELP;
            }
            if help {
                mapscreen::help_overlay(f, screen, help_lines);
            }
        });
    }
}

const DATABASE_HELP: &[&str] = &[
    "DATABASE  LEFT / RIGHT OR 1-5 TAB   UP / DOWN ENTRY   WHEEL, PGUP / PGDN SCROLL   ESC BACK",
    "          HOVER A VALUE: SOURCE, CONFIDENCE AND NOTES (CALC = DERIVED WITH THE SIM'S FORMULAS)",
];

const MENU_HELP: &[&str] =
    &["MENUS    UP / DOWN SELECT   ENTER OPEN   ESC BACK   MOUSE: HOVER AND CLICK", "ANYWHERE F11 FULLSCREEN"];

/// `--order`, `--bp`, `--pl`, `--plan`, `--select`/`--press`, `--advance`/`--code-word`,
/// `--save-replay`, `--aar`.
fn script(g: &mut Game, o: &Opts) -> Result<(), String> {
    for (kind, cs, n) in &o.orders {
        let platoon = g.picture().platoons.iter().find(|p| &p.callsign == cs).map(|p| p.id);
        match (kind.as_str(), platoon, n.as_slice()) {
            ("--bp", Some(p), [x, y, deg]) => {
                g.sel_platoon = Some(p);
                g.issue_bp([*x, *y], deg.to_radians());
            }
            ("--pl", _, pts) if pts.len() >= 4 && pts.len() % 2 == 0 => {
                g.add_phase_line(pts.chunks(2).map(|c| [c[0], c[1]]).collect());
            }
            // `--order <platoon>:x,y[,x,y..]`: checkpoints, then the destination.
            ("--order", Some(p), pts) if pts.len() >= 2 && pts.len() % 2 == 0 => {
                let mut pts: Vec<[f64; 2]> = pts.chunks(2).map(|c| [c[0], c[1]]).collect();
                let dest = pts.pop().expect("non-empty");
                g.sel_platoon = Some(p);
                g.issue_move(pts, dest);
            }
            ("--order", None, [x, y]) => {
                let id =
                    g.picture().own.iter().find(|u| &u.callsign == cs).map(|u| u.id).ok_or(format!("no unit {cs}"))?;
                g.order_move(id, [*x, *y]);
            }
            _ => return Err(format!("bad {kind} {cs}:{n:?}")),
        }
    }
    // `--select <platoon> --press F,W,T,H,S`: command-card keys, before `--advance`.
    // `--select <platoon or vehicle callsign>`.
    if let Some(cs) = &o.select {
        g.sel_platoon = g.picture().platoons.iter().find(|p| &p.callsign == cs).map(|p| p.id);
        g.sel_unit = match g.sel_platoon {
            Some(p) => g.picture().platoon(p).and_then(|v| v.members.first().copied()),
            None => g.picture().own.iter().find(|u| &u.callsign == cs).map(|u| u.id),
        };
    }
    if let Some(keys) = &o.press {
        for k in keys.split(',').map(str::trim) {
            match k {
                "F" => g.press(Cmd::Formation),
                "W" => g.press(Cmd::Weapons),
                "T" => g.press(Cmd::Technique),
                "H" => g.press(Cmd::Halt),
                "S" => g.press(Cmd::Speed),
                "Z" => g.press(Cmd::Smoke),
                "E" => g.press(Cmd::EngineSmoke),
                "X" => g.press(Cmd::BreakContact),
                "Q" => g.press(Cmd::Ammo),
                _ => {}
            }
        }
    }
    for (cs, spec) in &o.plans {
        let p =
            g.picture().platoons.iter().find(|p| &p.callsign == cs).map(|p| p.id).ok_or(format!("no platoon {cs}"))?;
        let plan = match spec.strip_prefix('@') {
            Some(path) => {
                let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
                sim_data::ron_options().from_str::<sim::view::Plan>(&text).map_err(|e| format!("{path}: {e}"))?
            }
            None => {
                let n: Vec<f64> = spec
                    .split(',')
                    .map(|x| x.trim().parse::<f64>().map_err(|_| format!("bad --plan {spec}")))
                    .collect::<Result<_, _>>()?;
                if n.is_empty() || !n.len().is_multiple_of(2) {
                    return Err(format!("bad --plan {spec}"));
                }
                let steps = n.chunks(2).enumerate().map(|(i, c)| sim::view::PlanStep::new(i as u16 + 1, [c[0], c[1]]));
                sim::view::Plan { steps: steps.collect() }
            }
        };
        g.sel_platoon = Some(p);
        g.sel_unit = g.picture().platoon(p).and_then(|v| v.members.first().copied());
        g.edit_plan(p, plan, !o.plan_draft);
    }
    for step in &o.timeline {
        match step {
            Step::Advance(s) => {
                g.execute();
                g.advance_ticks((s * sim::TICK_HZ as f64) as u32);
            }
            Step::CodeWord(name) => {
                let w = g.code_word_index(name).ok_or(format!("no code word {name}"))?;
                g.execute();
                g.act_now(game::GameAction::CodeWord(w));
            }
        }
    }
    if let Some(path) = &o.save_replay {
        std::fs::write(path, sim::Replay::record(&g.sim).to_ron()).map_err(|e| format!("{path}: {e}"))?;
    }
    if o.aar {
        g.open_aar();
    }
    Ok(())
}
