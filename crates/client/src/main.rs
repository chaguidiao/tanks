//! Tanks client: tactical map display, game mode and terrain editor.
//!
//! ```text
//! client [--scenario <id>] [--map <id>] [--edit] [--scenario-editor]
//!        [--screenshot <png> [--frames <n>] [--order <unit-or-platoon>:<x>,<y>] [--bp <platoon>:<x>,<y>,<deg>]
//!         [--pl -:<x>,<y>,<x>,<y>..] [--advance <s>] [--aar] [--save-replay <ron>]
//!         [--zoom <m-per-px>] [--center <x>,<y>] [--layer relief|slope|traffic|cover]]
//! ```

mod aar;
mod editor;
mod format;
mod game;
mod render;
mod scenedit;
mod style;
mod symbols;
mod view;

use std::path::PathBuf;
use std::sync::Arc;

use sim_data::{DataSet, MapRef};
use terragen::workflow;
use terrain::{AuthoringPackage, BakedMap};

use crate::editor::Editor;
use crate::game::Game;
use crate::render::{Gfx, Input, Key};
use crate::scenedit::ScenarioEditor;
use crate::view::{BaseLayer, Camera, MapView};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Game,
    Editor,
    Scenario,
}

struct Opts {
    scenario: Option<String>,
    map: Option<String>,
    edit: bool,
    scenario_editor: bool,
    aar: bool,
    save_replay: Option<String>,
    screenshot: Option<String>,
    frames: u32,
    /// ("--order" | "--bp", callsign, numbers)
    orders: Vec<(String, String, Vec<f64>)>,
    advance_s: f64,
    zoom: Option<f64>,
    center: Option<[f64; 2]>,
    layer: Option<String>,
}

fn parse_xy(s: &str) -> Option<[f64; 2]> {
    let (x, y) = s.split_once(',')?;
    Some([x.trim().parse().ok()?, y.trim().parse().ok()?])
}

fn opts() -> Result<Opts, String> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let get = |k: &str| a.iter().position(|x| x == k).and_then(|i| a.get(i + 1)).cloned();
    let mut orders = Vec::new();
    for (i, x) in a.iter().enumerate() {
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
        aar: a.iter().any(|x| x == "--aar"),
        save_replay: get("--save-replay"),
        screenshot: get("--screenshot"),
        frames: get("--frames").and_then(|v| v.parse().ok()).unwrap_or(6),
        orders,
        advance_s: get("--advance").and_then(|v| v.parse().ok()).unwrap_or(0.0),
        zoom: get("--zoom").and_then(|v| v.parse().ok()),
        center: get("--center").and_then(|v| parse_xy(&v)),
        layer: get("--layer"),
    })
}

struct App {
    gfx: Gfx,
    data_dir: PathBuf,
    ds: Arc<DataSet>,
    map_id: String,
    map: Arc<BakedMap>,
    view: MapView,
    cam: Camera,
    mode: Mode,
    game: Option<Game>,
    editor: Option<Editor>,
    scenario_editor: Option<ScenarioEditor>,
    map_changed: bool,
    help: bool,
    last_input: Input,
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
    let scenario = match (&o.scenario, &o.map) {
        (Some(s), _) => Some(DataSet::load_scenario(&data_dir, s).map_err(|e| e.to_string())?),
        (None, Some(_)) => None,
        (None, None) => Some(DataSet::load_scenario(&data_dir, "m1-move").map_err(|e| e.to_string())?),
    };
    let map_ref = match (&scenario, &o.map) {
        (Some(s), _) => s.map.clone(),
        (None, Some(id)) => MapRef { id: id.clone(), recipe: "rolling-farmland".into(), seed: 1, size_m: 8000.0 },
        _ => unreachable!(),
    };
    let map =
        Arc::new(workflow::ensure_map(&data_dir, &ds, &map_ref, |s| eprintln!("{s}")).map_err(|e| e.to_string())?);

    let gfx = Gfx::open(1600, 900, "TANKS", o.screenshot.is_some());
    let traffic_class = scenario
        .as_ref()
        .and_then(|s| s.sides.first())
        .and_then(|s| s.units.first())
        .map_or("bulwark_a1".to_owned(), |u| u.vehicle.clone());
    let mut view = MapView::new(&map, &traffic_class);
    if let Some(l) = &o.layer {
        view.base = match l.as_str() {
            "slope" => BaseLayer::Slope,
            "traffic" => BaseLayer::Trafficability,
            "cover" => BaseLayer::LandCover,
            _ => BaseLayer::Tactical,
        };
    }
    let mut cam = Camera { centre: [0.0, 0.0], mpp: 1.0, screen: [1600.0, 900.0] };
    cam.fit(map.manifest.width_m(), map.manifest.height_m());
    if let Some(z) = o.zoom {
        cam.mpp = z;
    }
    if let Some(c) = o.center {
        cam.centre = c;
    }

    let mut game = match scenario {
        Some(s) => Some(Game::new(s, ds.clone(), map.clone())?),
        None => None,
    };
    if let Some(g) = &mut game {
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
                ("--order", Some(p), [x, y]) => {
                    let t = g.technique;
                    g.sim.submit(
                        g.side,
                        sim::Command::PlatoonMove { platoon: p, destination: [*x, *y], formation: None, technique: t },
                    );
                }
                ("--order", None, [x, y]) => {
                    let id = g
                        .picture()
                        .own
                        .iter()
                        .find(|u| &u.callsign == cs)
                        .map(|u| u.id)
                        .ok_or(format!("no unit {cs}"))?;
                    g.order_move(id, [*x, *y]);
                }
                _ => return Err(format!("bad {kind} {cs}:{n:?}")),
            }
        }
        if o.advance_s > 0.0 {
            g.execute();
            g.advance_ticks((o.advance_s * sim::TICK_HZ as f64) as u32);
        }
        if let Some(path) = &o.save_replay {
            std::fs::write(path, sim::Replay::record(&g.sim).to_ron()).map_err(|e| format!("{path}: {e}"))?;
        }
        if o.aar {
            g.open_aar();
        }
    }

    let mut app = App {
        gfx,
        data_dir,
        ds,
        map_id: map_ref.id.clone(),
        map,
        view,
        cam,
        mode: Mode::Game,
        game,
        editor: None,
        scenario_editor: None,
        map_changed: false,
        help: false,
        last_input: Input::default(),
    };
    if o.edit || app.game.is_none() {
        app.enter_editor()?;
    } else if o.scenario_editor
        && let Some(g) = &app.game
    {
        app.scenario_editor = Some(ScenarioEditor::new(g.scenario.clone(), &app.ds));
        app.mode = Mode::Scenario;
    }

    let mut frame = 0u32;
    while !app.gfx.should_close() {
        app.tick()?;
        frame += 1;
        if let Some(path) = &o.screenshot
            && frame >= o.frames
        {
            app.gfx.screenshot(path);
            break;
        }
    }
    gfx_drop_guard(app);
    Ok(())
}

/// Textures must be released before the window closes; dropping the app in field order does it.
fn gfx_drop_guard(app: App) {
    let App { view, gfx, .. } = app;
    drop(view);
    drop(gfx);
}

impl App {
    fn enter_editor(&mut self) -> Result<(), String> {
        if self.editor.is_none() {
            let dir = workflow::map_dir(&self.data_dir, &self.map_id);
            let pkg = AuthoringPackage::load(&dir).map_err(|e| e.to_string())?;
            self.editor = Some(Editor::new(pkg));
        }
        if let Some(g) = &mut self.game {
            g.paused = true;
        }
        self.mode = Mode::Editor;
        Ok(())
    }

    fn leave_editor(&mut self) -> Result<(), String> {
        if self.game.is_none() {
            return Ok(()); // map-only session: stay in the editor
        }
        self.mode = Mode::Game;
        if self.map_changed {
            // The scenario restarts on the edited map.
            let g = self.game.take().expect("game exists");
            let mut ng = Game::new(g.scenario.clone(), self.ds.clone(), self.map.clone())?;
            ng.log.push_back((String::new(), "MAP CHANGED. SCENARIO RESTARTED.".into()));
            self.game = Some(ng);
            self.map_changed = false;
        }
        Ok(())
    }

    fn rebake(&mut self) {
        if let Some(ed) = &mut self.editor {
            let classes = workflow::mobility_classes(&self.ds);
            let baked = editor::rebake(&ed.pkg, &self.ds.terrain, &classes);
            self.map = Arc::new(baked);
            self.view.invalidate(&self.map);
            self.map_changed = true;
            ed.status = "BAKED".into();
        }
    }

    fn tick(&mut self) -> Result<(), String> {
        let inp = self.gfx.input();
        self.cam.screen = inp.screen;
        let over_ui = match self.mode {
            Mode::Game => self.game.as_ref().is_some_and(|g| g.over_ui(inp.mouse, inp.screen)),
            Mode::Editor => self.editor.as_ref().is_some_and(|e| e.over_ui(inp.mouse)),
            Mode::Scenario => self.scenario_editor.as_ref().is_some_and(|e| e.over_ui(inp.mouse)),
        };

        // Camera: wheel zooms about the cursor; middle-drag or arrow keys pan.
        if inp.wheel != 0.0 {
            self.cam.zoom_at(inp.mouse, if inp.wheel > 0.0 { 1.0 / 1.25 } else { 1.25 });
        }
        if inp.middle_down {
            self.cam.pan_px(inp.mouse_delta);
        }
        let pan = 600.0 * inp.frame_time;
        for (k, d) in
            [(Key::Left, [pan, 0.0]), (Key::Right, [-pan, 0.0]), (Key::Up, [0.0, pan]), (Key::Down, [0.0, -pan])]
        {
            if inp.down(k) {
                self.cam.pan_px(d);
            }
        }

        // Layers (F1–F4 base, F5 contours, F6 grid), help (F7), mode (E).
        for (n, b) in
            [(1, BaseLayer::Tactical), (2, BaseLayer::Slope), (3, BaseLayer::Trafficability), (4, BaseLayer::LandCover)]
        {
            if inp.hit(Key::F(n)) {
                self.view.base = b;
            }
        }
        if inp.hit(Key::F(5)) {
            self.view.contours = !self.view.contours;
        }
        if inp.hit(Key::F(6)) {
            self.view.grid = !self.view.grid;
        }
        if inp.hit(Key::F(7)) {
            self.help = !self.help;
        }
        if inp.hit(Key::Char('e')) && !inp.ctrl {
            match self.mode {
                Mode::Game => self.enter_editor()?,
                Mode::Editor => self.leave_editor()?,
                Mode::Scenario => {}
            }
        }
        if inp.hit(Key::F(9))
            && self.mode == Mode::Game
            && let Some(g) = &mut self.game
        {
            g.paused = true;
            self.scenario_editor = Some(ScenarioEditor::new(g.scenario.clone(), &self.ds));
            self.mode = Mode::Scenario;
            self.last_input = inp;
            self.draw();
            return Ok(());
        }

        match self.mode {
            Mode::Game => {
                if let Some(g) = &mut self.game {
                    let fx = g.update(&inp, &self.cam, over_ui);
                    if fx.clear_overlay {
                        self.view.overlay = None;
                    }
                    if let Some((rgba, w, h, ox, oy, step)) = fx.viewshed {
                        let tex = self.gfx.texture(w, h, &rgba);
                        self.view.overlay = Some((tex, [ox, oy, ox + w as f64 * step, oy + h as f64 * step]));
                    }
                }
            }
            Mode::Editor => {
                let fx = {
                    let map = Arc::make_mut(&mut self.map);
                    let ed = self.editor.as_mut().expect("editor exists in editor mode");
                    ed.update(&inp, &self.cam, map, over_ui)
                };
                if let Some(r) = fx.preview {
                    self.view.refresh_rect(&mut self.gfx, &self.map, &self.ds.terrain, r);
                    self.map_changed = true;
                }
                if fx.rebake {
                    self.rebake();
                }
                if fx.save {
                    self.save();
                }
            }
            Mode::Scenario => {
                let ed = self.scenario_editor.as_mut().expect("scenario editor exists in scenario mode");
                let fx = ed.update(&inp, &self.cam, &self.ds, over_ui);
                if fx.save {
                    match sim_data::DataSet::save_scenario(&self.data_dir, &ed.def) {
                        Ok(p) => {
                            ed.unsaved = false;
                            ed.status = format!("SAVED {}", p.display());
                        }
                        Err(e) => ed.status = format!("SAVE FAILED: {e}"),
                    }
                }
                if fx.play {
                    let def = ed.def.clone();
                    match Game::new(def, self.ds.clone(), self.map.clone()) {
                        Ok(mut g) => {
                            g.log.push_back((String::new(), "SCENARIO EDITED. RESTARTED.".into()));
                            self.game = Some(g);
                            self.scenario_editor = None;
                            self.mode = Mode::Game;
                        }
                        Err(e) => ed.status = format!("CANNOT START: {e}"),
                    }
                }
            }
        }

        self.view.ensure(&mut self.gfx, &self.map, &self.ds.terrain);
        self.last_input = inp;
        self.draw();
        Ok(())
    }

    fn save(&mut self) {
        let Some(ed) = &mut self.editor else { return };
        let dir = workflow::map_dir(&self.data_dir, &self.map_id);
        let r = ed.pkg.save(&dir).and_then(|_| {
            let classes = workflow::mobility_classes(&self.ds);
            let baked = editor::rebake(&ed.pkg, &self.ds.terrain, &classes);
            baked.save(&dir.join("baked"))?;
            Ok(baked)
        });
        match r {
            Ok(b) => {
                self.map = Arc::new(b);
                self.view.invalidate(&self.map);
                ed.unsaved = false;
                ed.status = format!("SAVED {}", dir.display());
            }
            Err(e) => ed.status = format!("SAVE FAILED: {e}"),
        }
    }

    fn draw(&mut self) {
        let inp = &self.last_input;
        let (cam, view, map, ds) = (&self.cam, &self.view, &self.map, &self.ds);
        let (mode, game, editor, help) = (self.mode, &self.game, &self.editor, self.help);
        let scenario_editor = &self.scenario_editor;
        self.gfx.frame(|f| {
            f.clear(style::BG);
            view.draw(f, cam, map);
            if mode != Mode::Scenario
                && let Some(g) = game
            {
                g.draw_world(f, cam, inp.mouse);
            }
            if let Some(e) = scenario_editor.as_ref().filter(|_| mode == Mode::Scenario) {
                e.draw_world(f, cam);
            }
            if mode == Mode::Editor
                && let Some(e) = editor
            {
                e.draw_world(f, cam, inp.mouse);
            }
            match mode {
                Mode::Game => {
                    if let Some(g) = game {
                        g.draw_ui(f, inp.screen);
                    }
                }
                Mode::Editor => {
                    if let Some(e) = editor {
                        e.draw_ui(f, inp.screen);
                    }
                }
                Mode::Scenario => {
                    if let Some(e) = scenario_editor {
                        e.draw_ui(f, inp.screen, ds);
                    }
                }
            }
            status_line(f, inp, cam, map, ds, view.base);
            if help {
                help_overlay(f, inp.screen);
            }
        });
    }
}

fn status_line(f: &mut render::Frame, inp: &Input, cam: &Camera, map: &BakedMap, ds: &DataSet, base: BaseLayer) {
    let p = cam.to_world(inp.mouse);
    let (w, h) = (map.manifest.width_m(), map.manifest.height_m());
    let y = inp.screen[1] - 20.0;
    f.rect(0.0, y - 4.0, inp.screen[0], 24.0, style::PANEL);
    let mut s = String::new();
    if p[0] >= 0.0 && p[1] >= 0.0 && p[0] < w && p[1] < h {
        let hf = &map.elevation;
        let (i, j) = hf.cell_of(p[0], p[1]);
        let lc = ds.terrain.landcover(map.landcover.get(i, j));
        s = format!(
            "GRID {}   ELEV {:.1} M   SLOPE {:.0} %   {}",
            format::grid8(p),
            hf.height(p[0], p[1]),
            hf.node_slope(i, j) * 100.0,
            lc.name.to_uppercase().replace('_', " ")
        );
        let wd = map.water_depth.get(i, j);
        if wd > 0.05 {
            s.push_str(&format!("   WATER {wd:.1} M"));
        }
    }
    f.text(&s, 10.0, y, style::FONT_L - 8, style::TEXT);
    let right = format!("{}   {:.1} M/PX   F7 HELP", base.label(), cam.mpp);
    let tw = f.text_width(&right, style::FONT_L - 8);
    f.text(&right, inp.screen[0] - tw - 10.0, y, style::FONT_L - 8, style::TEXT_DIM);
}

fn help_overlay(f: &mut render::Frame, screen: [f32; 2]) {
    let lines = [
        "MAP     WHEEL ZOOM   MMB DRAG / ARROWS PAN",
        "LAYERS  F1 RELIEF  F2 SLOPE  F3 GO/NO-GO  F4 LAND COVER  F5 CONTOURS  F6 GRID",
        "GAME    LMB SELECT PLATOON (CTRL: SINGLE TANK)  RMB MOVE  CTRL+RMB ENGAGE CONTACT  TAB NEXT PLATOON",
        "        F FORMATION  T TECHNIQUE  B BATTLE POSITION ([ ] RADIUS)  W WEAPONS CONTROL  H HALT",
        "        SPACE PAUSE  1-4 SPEED X1 X2 X4 X8   V VIEWSHED  L LINE OF SIGHT",
        "PLAN    ENTER EXECUTE (PLANNING PHASE)  G GRAPHICS (PL/OBJ/TRP)  K CONTINGENCY MODE FOR NEXT ORDER",
        "        F8 AFTER-ACTION REVIEW (CTRL+R SAVE REPLAY)",
        "SCENARIO F9 EDIT/PLAY  P PLACE  LMB DRAG  R ROTATE  DEL REMOVE  TAB SIDE  V VEHICLE  N PLATOON",
        "        O OBJECTIVE  Y/U YEAR  T START TIME  CTRL+S SAVE",
        "EDITOR  E TOGGLE  1-4 BRUSHES  5 RIVER  6 ROAD  7 FOREST  8 BUILDING  DEL DELETE  [ ] RADIUS  - = RATE",
        "        CTRL+Z/Y UNDO/REDO  B BAKE  CTRL+S SAVE",
    ];
    let (w, h) = (900.0, lines.len() as f32 * 20.0 + 20.0);
    let (x, y) = ((screen[0] - w) / 2.0, (screen[1] - h) / 2.0);
    f.rect(x, y, w, h, style::PANEL);
    f.rect_lines(x, y, w, h, style::PANEL_EDGE);
    for (k, l) in lines.iter().enumerate() {
        f.text(l, x + 12.0, y + 12.0 + k as f32 * 20.0, style::FONT_L - 8, style::TEXT);
    }
}
