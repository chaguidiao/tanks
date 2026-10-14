//! Map screens: the game, the terrain editor and the scenario editor (DESIGN.md §10.4). Each
//! owns its map, view and camera and has its own top bar and help; the app (main.rs) switches
//! between them and the menu screens. Esc (or F10) asks the app for the screen menu.

use std::path::Path;
use std::sync::Arc;

use sim_data::DataSet;
use terragen::workflow;
use terrain::{AuthoringPackage, BakedMap};

use crate::editor::{self, BakeEvent, BakeQueue, Editor};
use crate::game::{Game, GameAction};
use crate::hud::{self, Button, Look, Rect};
use crate::render::{Frame, Gfx, Input, Key, P2};
use crate::scenedit::ScenarioEditor;
use crate::settings::Settings;
use crate::style;
use crate::view::{BaseLayer, Camera, MapView, Minimap, build_layer};
use crate::{format, render};

/// What the app lends a map screen each frame.
pub struct Ctx<'a> {
    pub ds: &'a Arc<DataSet>,
    pub data_dir: &'a Path,
    pub settings: &'a Settings,
}

/// Top-bar, end-of-mission and screen-menu actions of a map screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenAction {
    Layer(BaseLayer),
    Contours,
    Grid,
    Help,
    Menu,
    Bake,
    Save,
    PlayTest,
    BackToEditor,
    Restart,
    Aar,
    MainMenu,
    CloseEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Clicked {
    Screen(ScreenAction),
    Game(GameAction),
}

/// What a map screen asks the app to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Menu,
    Help,
    /// DATABASE on this vehicle type (the game's selected-vehicle card).
    VehicleData(String),
    MainMenu,
    /// Scenario editor → game on the edited scenario.
    PlayTest,
    /// Play test → back to the scenario editor.
    BackToEditor,
}

/// Background re-bake result: the baked map plus the image of the layer that was on screen.
pub struct Baked {
    map: BakedMap,
    base: BaseLayer,
    img: Vec<u8>,
}

pub enum Mode {
    Game {
        game: Box<Game>,
        /// The scenario editor this play test was started from.
        from_editor: Option<Box<ScenarioEditor>>,
        /// The end-of-mission panel was closed (keep looking at the map).
        end_closed: bool,
    },
    Terrain {
        editor: Box<Editor>,
        /// Bakes run on a worker thread so the editor stays responsive.
        bake: BakeQueue<Baked>,
    },
    Scenario {
        editor: Box<ScenarioEditor>,
    },
}

pub struct MapScreen {
    pub map_id: String,
    pub map: Arc<BakedMap>,
    /// Holds textures: the app drops screens before the window (field order in `App`).
    pub view: MapView,
    pub cam: Camera,
    pub mode: Mode,
}

fn traffic_class(def: &sim_data::ScenarioDef) -> String {
    def.sides.first().and_then(|s| s.units.first()).map_or("bulwark_a1".to_owned(), |u| u.vehicle.clone())
}

impl MapScreen {
    fn new(map_id: String, map: Arc<BakedMap>, class: &str, mode: Mode, settings: &Settings) -> Self {
        let mut view = MapView::new(&map, class);
        view.contours = settings.contours;
        view.grid = settings.grid;
        let mut cam = Camera { centre: [0.0, 0.0], mpp: 1.0, screen: [1600.0, 900.0] };
        cam.fit(map.manifest.width_m(), map.manifest.height_m());
        Self { map_id, map, view, cam, mode }
    }

    pub fn game(map: Arc<BakedMap>, def: sim_data::ScenarioDef, ctx: &Ctx) -> Result<Self, String> {
        let (id, class) = (def.map.id.clone(), traffic_class(&def));
        let game = Box::new(Game::new(def, ctx.ds.clone(), map.clone())?);
        let mode = Mode::Game { game, from_editor: None, end_closed: false };
        Ok(Self::new(id, map, &class, mode, ctx.settings))
    }

    pub fn scenario_editor(map: Arc<BakedMap>, def: sim_data::ScenarioDef, ctx: &Ctx) -> Self {
        let (id, class) = (def.map.id.clone(), traffic_class(&def));
        let editor = Box::new(ScenarioEditor::new(def, ctx.ds));
        Self::new(id, map, &class, Mode::Scenario { editor }, ctx.settings)
    }

    pub fn terrain_editor(map_id: String, map: Arc<BakedMap>, pkg: AuthoringPackage, ctx: &Ctx) -> Self {
        let class = ctx.ds.vehicles.first().map_or("bulwark_a1".to_owned(), |v| v.id.clone());
        let mode = Mode::Terrain { editor: Box::new(Editor::new(pkg)), bake: BakeQueue::default() };
        Self::new(map_id, map, &class, mode, ctx.settings)
    }

    pub fn game_mut(&mut self) -> Option<&mut Game> {
        match &mut self.mode {
            Mode::Game { game, .. } => Some(game.as_mut()),
            _ => None,
        }
    }

    /// Screen title (top bar, menus).
    pub fn title(&self) -> String {
        match &self.mode {
            Mode::Game { game, .. } => game.scenario.name.to_uppercase(),
            Mode::Terrain { .. } => format!("TERRAIN EDITOR: MAP {}", self.map_id.to_uppercase()),
            Mode::Scenario { editor } => format!("SCENARIO EDITOR: {}", editor.def.name.to_uppercase()),
        }
    }

    /// Edits that leaving the screen would lose.
    pub fn unsaved(&self) -> bool {
        match &self.mode {
            Mode::Game { from_editor, .. } => from_editor.as_ref().is_some_and(|e| e.unsaved),
            Mode::Terrain { editor, .. } => editor.unsaved,
            Mode::Scenario { editor } => editor.unsaved,
        }
    }

    /// Entries of the screen menu (Esc), after RESUME and before OPTIONS / MAIN MENU / QUIT.
    pub fn menu_items(&self) -> Vec<(&'static str, ScreenAction, &'static str)> {
        match &self.mode {
            Mode::Game { game, from_editor, .. } => {
                let mut v = vec![
                    ("AFTER-ACTION REVIEW", ScreenAction::Aar, "F8"),
                    ("RESTART SCENARIO", ScreenAction::Restart, ""),
                ];
                if game.aar.is_some() {
                    v[0].0 = "CLOSE AFTER-ACTION REVIEW";
                }
                if from_editor.is_some() {
                    v.push(("BACK TO SCENARIO EDITOR", ScreenAction::BackToEditor, "F9"));
                }
                v
            }
            Mode::Terrain { .. } => {
                vec![("SAVE MAP", ScreenAction::Save, "CTRL+S"), ("BAKE NOW", ScreenAction::Bake, "B")]
            }
            Mode::Scenario { .. } => {
                vec![("SAVE SCENARIO", ScreenAction::Save, "CTRL+S"), ("PLAY TEST", ScreenAction::PlayTest, "F9")]
            }
        }
    }

    /// Scenario editor → play test (the editor is kept for the way back). If the scenario
    /// cannot start, the editor stays and shows why.
    pub fn play_test(self, ctx: &Ctx) -> Self {
        let Self { map_id, map, view, cam, mode } = self;
        let Mode::Scenario { mut editor } = mode else {
            return Self { map_id, map, view, cam, mode };
        };
        let mode = match Game::new(editor.def.clone(), ctx.ds.clone(), map.clone()) {
            Ok(mut g) => {
                g.radio("PLAY TEST. F9 RETURNS TO THE SCENARIO EDITOR.".into());
                Mode::Game { game: Box::new(g), from_editor: Some(editor), end_closed: false }
            }
            Err(e) => {
                editor.status = format!("CANNOT START: {e}");
                Mode::Scenario { editor }
            }
        };
        Self { map_id, map, view, cam, mode }
    }

    /// Play test → scenario editor.
    pub fn back_to_editor(self) -> Self {
        let Self { map_id, map, mut view, cam, mode } = self;
        let mode = match mode {
            Mode::Game { from_editor: Some(editor), .. } => Mode::Scenario { editor },
            m => m,
        };
        view.overlay = None;
        Self { map_id, map, view, cam, mode }
    }

    /// Esc would cancel something on this screen rather than open the menu.
    fn esc_in_use(&self) -> bool {
        match &self.mode {
            Mode::Game { game, .. } => game.esc_in_use(),
            Mode::Terrain { editor, .. } => editor.busy(),
            Mode::Scenario { .. } => false,
        }
    }

    /// Minimap (game mode only, not in the after-action review).
    fn minimap(&self, screen: P2) -> Option<Minimap> {
        match &self.mode {
            Mode::Game { game, .. } if game.aar.is_none() => Some(Minimap::new(
                hud::minimap_frame(screen),
                self.map.manifest.width_m(),
                self.map.manifest.height_m(),
            )),
            _ => None,
        }
    }

    /// End-of-mission panel frame, if it is showing.
    fn end_panel(&self, screen: P2) -> Option<Rect> {
        match &self.mode {
            Mode::Game { game, end_closed: false, .. } if game.ended.is_some() && game.aar.is_none() => {
                let w = 600.0;
                Some(Rect::new((screen[0] - w) / 2.0, hud::TOP_H + 90.0, w, 120.0))
            }
            _ => None,
        }
    }

    /// Top-bar and panel buttons of the screen, and every game-mode button.
    fn buttons(&self, screen: P2) -> (Vec<Button<ScreenAction>>, Vec<Button<GameAction>>) {
        let z = Rect::new(0.0, 0.0, 0.0, 0.0);
        let b = |l: &str, a: ScreenAction| Button::new(z, Look::Bar, l, a);
        let base = self.view.base;
        let mut items = vec![
            b("RELIEF", ScreenAction::Layer(BaseLayer::Tactical))
                .key("F1")
                .tip("BASE MAP: SHADED RELIEF AND LAND COVER")
                .active(base == BaseLayer::Tactical),
            b("SLOPE", ScreenAction::Layer(BaseLayer::Slope))
                .key("F2")
                .tip("BASE MAP: SLOPE")
                .active(base == BaseLayer::Slope),
            b("GO/NO-GO", ScreenAction::Layer(BaseLayer::Trafficability))
                .key("F3")
                .tip("BASE MAP: TRAFFICABILITY FOR THE FIRST VEHICLE TYPE")
                .active(base == BaseLayer::Trafficability),
            b("COVER", ScreenAction::Layer(BaseLayer::LandCover))
                .key("F4")
                .tip("BASE MAP: LAND COVER CLASSES")
                .active(base == BaseLayer::LandCover),
            b("CONTOURS", ScreenAction::Contours).key("F5").tip("CONTOUR LINES").active(self.view.contours),
            b("GRID", ScreenAction::Grid).key("F6").tip("1 KM GRID").active(self.view.grid),
        ];
        match &self.mode {
            Mode::Game { from_editor: Some(_), .. } => {
                items.push(b("EDITOR", ScreenAction::BackToEditor).key("F9").tip("BACK TO THE SCENARIO EDITOR"));
            }
            Mode::Game { .. } => {}
            Mode::Terrain { editor, .. } => {
                items.push(b("BAKE", ScreenAction::Bake).key("B").tip("RE-BAKE THE MAP NOW (ALSO AUTOMATIC)"));
                items.push(
                    b("SAVE", ScreenAction::Save)
                        .key("CTRL+S")
                        .tip("SAVE THE AUTHORING PACKAGE AND THE BAKE")
                        .active(editor.unsaved),
                );
            }
            Mode::Scenario { editor } => {
                items.push(
                    b("SAVE", ScreenAction::Save)
                        .key("CTRL+S")
                        .tip("SAVE AS DATA/SCENARIOS/<ID>-CUSTOM.RON")
                        .active(editor.unsaved),
                );
                items.push(b("PLAY TEST", ScreenAction::PlayTest).key("F9").tip("PLAY THE EDITED SCENARIO"));
            }
        }
        items.push(b("HELP", ScreenAction::Help).key("F7").tip("KEYBOARD AND MOUSE REFERENCE"));
        items.push(b("MENU", ScreenAction::Menu).key("ESC").tip("MENU: OPTIONS, MAIN MENU, QUIT"));
        let x = screen[0] - hud::bar_width(&items) - 6.0;
        let mut out = Vec::new();
        hud::bar_row(&mut out, x, items);
        if let Some(p) = self.end_panel(screen) {
            let labels = [
                ("AFTER-ACTION REVIEW", ScreenAction::Aar, "F8"),
                ("RESTART", ScreenAction::Restart, ""),
                ("MAIN MENU", ScreenAction::MainMenu, ""),
                ("CLOSE", ScreenAction::CloseEnd, ""),
            ];
            let (gap, bw) = (8.0, (p.w - 40.0 - 3.0 * 8.0) / 4.0);
            for (k, (l, a, key)) in labels.into_iter().enumerate() {
                let r = Rect::new(p.x + 20.0 + k as f32 * (bw + gap), p.y + p.h - 44.0, bw, 28.0);
                out.push(Button::new(r, Look::Bar, l, a).key(key));
            }
        }
        let game = match &self.mode {
            Mode::Game { game, .. } => game.buttons(screen),
            _ => Vec::new(),
        };
        (out, game)
    }

    pub fn help_lines(&self) -> &'static [&'static str] {
        const MAP: &str = "MAP      WHEEL ZOOM   MMB DRAG / ARROWS PAN   F1-F4 BASE MAP  F5 CONTOURS  F6 GRID";
        match &self.mode {
            Mode::Game { .. } => &[
                "EVERY BUTTON SHOWS ITS HOTKEY; HOVER FOR A DESCRIPTION.   ESC / F10 MENU",
                "",
                MAP,
                "         CLICK/DRAG MINIMAP TO MOVE",
                "SELECT   LMB PLATOON (CTRL: SINGLE TANK)  ROSTER / PANEL ROWS (CLICK AGAIN: CENTRE)  TAB NEXT",
                "         SINGLE TANK: STATUS CARD (CREW, FUEL, ROUNDS, GUN, TARGET, SIGHTS)   I VEHICLE DATA (DATABASE)",
                "MOVE     RMB MOVE   M MOVE TOOL   SHIFT+RMB ADD CHECKPOINT (RELEASE SHIFT TO SEND, APPENDS TO ROUTE)",
                "MENU     HOLD RMB (OR RMB ON A UNIT / CONTACT / GRAPHIC) FOR THE CONTEXT MENU: ARROWS, ENTER, ESC, LETTERS",
                "         R ROUTE TOOL: LMB CHECKPOINTS, RMB DESTINATION, BKSP UNDO   H HALT / RESUME",
                "ORDERS   F FORMATION  T TECHNIQUE  S MOVE SPEED  B BATTLE POSITION ([ ] RADIUS)  W WEAPONS  A / CTRL+RMB ENGAGE",
                "TIME     SPACE PAUSE  1-4 SPEED X1 X2 X4 X8   V VIEWSHED  L LINE OF SIGHT",
                "CP PLAN  P PLAN: LMB ADDS CPS, DRAG A CP, RMB A CP / LEG: LEG, ACTIONS, WAIT, BRANCH   , . CP   C CP MENU",
                "         DEL DELETE CP   ENTER SEND   ESC REVERT   CTRL+1-9 CODE WORD (LEFT PANEL: WHO IS SET)",
                "STAFF    ENTER EXECUTE (PLANNING PHASE)  G GRAPHICS (PL/OBJ/TRP)  K TIMING OF THE NEXT ORDER",
                "         RMB OR ESC CANCELS A TOOL   F8 AFTER-ACTION REVIEW (CTRL+R SAVE REPLAY)",
                "         F9 BACK TO THE SCENARIO EDITOR (PLAY TEST)",
            ],
            Mode::Terrain { .. } => &[
                "TERRAIN EDITOR: EDITS THE AUTHORING PACKAGE; THE MAP RE-BAKES IN THE BACKGROUND.   ESC / F10 MENU",
                "",
                MAP,
                "BRUSHES  1 RAISE  2 LOWER  3 SMOOTH  4 FLATTEN   LMB PAINT   [ ] RADIUS   - = RATE",
                "DRAW     5 RIVER  6 ROAD  7 FOREST: LMB POINTS, ENTER FINISH, BKSP UNDO POINT, ESC CANCEL",
                "         8 BUILDING: LMB CENTRE, LMB END OF LONG AXIS   DEL DELETE FEATURE UNDER CURSOR",
                "EDIT     CTRL+Z / CTRL+Y UNDO / REDO   B BAKE NOW   CTRL+S SAVE",
                "",
                "SCENARIOS ON THIS MAP LOAD THE SAVED MAP THE NEXT TIME THEY START.",
            ],
            Mode::Scenario { .. } => &[
                "SCENARIO EDITOR: UNITS, OBJECTIVES, DATE AND START TIME.   ESC / F10 MENU",
                "",
                MAP,
                "UNITS    P PLACE AT CURSOR  LMB SELECT / DRAG  R ROTATE 45°  DEL REMOVE",
                "         TAB SIDE  V VEHICLE (IN SERVICE AT THE DATE)  N NEW PLATOON",
                "OBJ      O OBJECTIVE AT CURSOR  [ ] RADIUS OF THE SELECTED OBJECTIVE",
                "DATE     Y / U YEAR -/+   T START TIME +3 H",
                "FILE     CTRL+S SAVE (AS <ID>-CUSTOM)   F9 PLAY TEST",
            ],
        }
    }

    /// Run one screen action (top bar, end panel or the screen menu).
    pub fn act(&mut self, a: ScreenAction, ctx: &Ctx) -> Option<Request> {
        match a {
            ScreenAction::Layer(b) => self.view.base = b,
            ScreenAction::Contours => self.view.contours = !self.view.contours,
            ScreenAction::Grid => self.view.grid = !self.view.grid,
            ScreenAction::Help => return Some(Request::Help),
            ScreenAction::Menu => return Some(Request::Menu),
            ScreenAction::MainMenu => return Some(Request::MainMenu),
            ScreenAction::PlayTest => return Some(Request::PlayTest),
            ScreenAction::BackToEditor => return Some(Request::BackToEditor),
            ScreenAction::Bake => {
                if let Mode::Terrain { bake, .. } = &mut self.mode {
                    bake.request();
                }
            }
            ScreenAction::Save => self.save(ctx),
            ScreenAction::Restart => self.restart(ctx),
            ScreenAction::Aar => {
                if let Mode::Game { game, .. } = &mut self.mode {
                    if game.aar.is_some() {
                        game.aar = None;
                    } else {
                        game.open_aar();
                    }
                }
            }
            ScreenAction::CloseEnd => {
                if let Mode::Game { end_closed, .. } = &mut self.mode {
                    *end_closed = true;
                }
            }
        }
        None
    }

    fn restart(&mut self, ctx: &Ctx) {
        if let Mode::Game { game, end_closed, .. } = &mut self.mode {
            match Game::new(game.scenario.clone(), ctx.ds.clone(), self.map.clone()) {
                Ok(mut g) => {
                    g.log.push_front((String::new(), "SCENARIO RESTARTED.".into()));
                    **game = g;
                    *end_closed = false;
                    self.view.overlay = None;
                }
                Err(e) => game.radio(format!("RESTART FAILED: {e}")),
            }
        }
    }

    fn save(&mut self, ctx: &Ctx) {
        match &mut self.mode {
            Mode::Terrain { editor: ed, bake } => {
                // Saving bakes the current package itself; a background bake would be older.
                bake.cancel();
                let dir = workflow::map_dir(ctx.data_dir, &self.map_id);
                let r = ed.pkg.save(&dir).and_then(|_| {
                    let baked = editor::rebake(&ed.pkg, &ctx.ds.terrain, &workflow::mobility_classes(ctx.ds));
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
            Mode::Scenario { editor: ed } => {
                ed.prepare_save();
                match DataSet::save_scenario(ctx.data_dir, &ed.def) {
                    Ok(p) => {
                        ed.unsaved = false;
                        ed.status = format!("SAVED {}", p.display());
                    }
                    Err(e) => ed.status = format!("SAVE FAILED: {e}"),
                }
            }
            Mode::Game { .. } => {}
        }
    }

    /// Start a background bake if one is wanted, and install a finished one.
    fn poll_bake(&mut self, gfx: &mut Gfx, ds: &Arc<DataSet>) {
        let Mode::Terrain { editor: ed, bake } = &mut self.mode else { return };
        let (base, class) = (self.view.base, &self.view.traffic_class);
        let ev = bake.poll(|| {
            let (pkg, ds, class) = (ed.pkg.clone(), ds.clone(), class.clone());
            move || {
                let map = editor::rebake(&pkg, &ds.terrain, &workflow::mobility_classes(&ds));
                let img = build_layer(base, &class, &map, &ds.terrain);
                Baked { map, base, img }
            }
        });
        match ev {
            BakeEvent::Idle => {}
            BakeEvent::Started => ed.status = "BAKING...".into(),
            BakeEvent::Failed => ed.status = "BAKE FAILED".into(),
            BakeEvent::Done(b) => {
                ed.status = "BAKED".into();
                self.map = Arc::new(b.map);
                self.view.replace_map(gfx, &self.map, b.base, &b.img);
            }
        }
    }

    /// One frame of input. Returns what the app should do (menu, help, screen change).
    pub fn update(&mut self, inp: &Input, gfx: &mut Gfx, ctx: &Ctx) -> Option<Request> {
        self.cam.screen = inp.screen;
        if inp.hit(Key::F(10)) || (inp.hit(Key::Escape) && !self.esc_in_use()) {
            return Some(Request::Menu);
        }
        let (mut screen_buttons, mut game_buttons) = self.buttons(inp.screen);
        // Buttons under an open context menu are not reachable.
        if let Mode::Game { game, .. } = &mut self.mode {
            game.pause_on_menu = ctx.settings.pause_on_menu;
            game.pause_on_ready = ctx.settings.pause_on_ready;
            if game.menu_frames(inp.screen).iter().any(|r| r.contains(inp.mouse)) {
                screen_buttons.clear();
                game_buttons.clear();
            }
        }
        let clicked = hud::clicked(&screen_buttons, inp.mouse, inp.left_pressed)
            .map(Clicked::Screen)
            .or_else(|| hud::clicked(&game_buttons, inp.mouse, inp.left_pressed).map(Clicked::Game));
        if let Some(Clicked::Screen(a)) = clicked
            && let Some(r) = self.act(a, ctx)
        {
            return Some(r);
        }
        let minimap = self.minimap(inp.screen);
        let in_minimap = minimap.is_some_and(|m| m.r.contains(inp.mouse));
        let over_ui = inp.mouse[1] < hud::TOP_H
            || inp.mouse[1] > inp.screen[1] - hud::STATUS_H
            || hud::hovered(&game_buttons, inp.mouse).is_some()
            || hud::hovered(&screen_buttons, inp.mouse).is_some()
            || self.end_panel(inp.screen).is_some_and(|r| r.contains(inp.mouse))
            || match &self.mode {
                Mode::Game { game, .. } => game.panels(inp.screen).iter().any(|r| r.contains(inp.mouse)),
                Mode::Terrain { editor, .. } => editor.over_ui(inp.mouse),
                Mode::Scenario { editor } => editor.over_ui(inp.mouse),
            };
        // Minimap: click or drag to move the view.
        if let Some(m) = minimap
            && in_minimap
            && (inp.left_pressed || inp.left_down)
        {
            self.cam.centre = m.to_world(inp.mouse);
        }
        self.camera(inp, ctx.settings);

        // Layers (F1–F4 base, F5 contours, F6 grid), help (F7).
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
            return Some(Request::Help);
        }

        let game_click = match clicked {
            Some(Clicked::Game(a)) => Some(a),
            _ => None,
        };
        let mut req = None;
        match &mut self.mode {
            Mode::Game { game, from_editor, .. } => {
                if inp.hit(Key::F(9)) && from_editor.is_some() {
                    return Some(Request::BackToEditor);
                }
                let fx = game.update(inp, &self.cam, over_ui || in_minimap, game_click);
                if let Some(c) = fx.center_on {
                    self.cam.centre = c;
                }
                if let Some(id) = fx.vehicle_data {
                    req = Some(Request::VehicleData(id));
                }
                if fx.clear_overlay {
                    self.view.overlay = None;
                }
                if let Some((rgba, w, h, ox, oy, step)) = fx.viewshed {
                    let tex = gfx.texture(w, h, &rgba);
                    self.view.overlay = Some((tex, [ox, oy, ox + w as f64 * step, oy + h as f64 * step]));
                }
            }
            Mode::Terrain { editor, bake } => {
                let fx = editor.update(inp, &self.cam, Arc::make_mut(&mut self.map), over_ui);
                if let Some(r) = fx.preview {
                    self.view.refresh_rect(gfx, &self.map, &ctx.ds.terrain, r);
                    bake.touch();
                }
                if fx.rebake {
                    bake.request();
                }
                self.poll_bake(gfx, ctx.ds);
                if fx.save {
                    self.save(ctx);
                }
            }
            Mode::Scenario { editor } => {
                let fx = editor.update(inp, &self.cam, ctx.ds, over_ui);
                if fx.save {
                    self.save(ctx);
                }
                if fx.play {
                    req = Some(Request::PlayTest);
                }
            }
        }
        req
    }

    /// Build missing map textures (every frame, also while a menu covers the screen).
    pub fn ensure_view(&mut self, gfx: &mut Gfx, ds: &DataSet) {
        self.view.ensure(gfx, &self.map, &ds.terrain);
    }

    /// Wheel zooms about the cursor; middle-drag, arrow keys or the screen edge pan.
    fn camera(&mut self, inp: &Input, settings: &Settings) {
        if inp.wheel != 0.0 {
            self.cam.zoom_at(inp.mouse, if inp.wheel > 0.0 { 1.0 / 1.25 } else { 1.25 });
        }
        if inp.middle_down {
            self.cam.pan_px(inp.mouse_delta);
        }
        let pan = settings.pan_speed * inp.frame_time;
        for (k, d) in
            [(Key::Left, [pan, 0.0]), (Key::Right, [-pan, 0.0]), (Key::Up, [0.0, pan]), (Key::Down, [0.0, -pan])]
        {
            if inp.down(k) {
                self.cam.pan_px(d);
            }
        }
        if settings.edge_pan {
            // A 3 px band: free of the top bar's buttons, which start 4 px down.
            let (m, s) = (inp.mouse, inp.screen);
            let e = 3.0;
            let dx = if m[0] < e {
                pan
            } else if m[0] > s[0] - e {
                -pan
            } else {
                0.0
            };
            let dy = if m[1] < e {
                pan
            } else if m[1] > s[1] - e {
                -pan
            } else {
                0.0
            };
            if dx != 0.0 || dy != 0.0 {
                self.cam.pan_px([dx, dy]);
            }
        }
    }

    pub fn draw(&self, f: &mut Frame, inp: &Input, ds: &DataSet) {
        let (screen_buttons, game_buttons) = self.buttons(inp.screen);
        let minimap = self.minimap(inp.screen);
        let (cam, view, map) = (&self.cam, &self.view, &self.map);
        f.clear(style::BG);
        view.draw(f, cam, map);
        match &self.mode {
            Mode::Game { game, .. } => game.draw_world(f, cam, inp.mouse),
            Mode::Terrain { editor, .. } => editor.draw_world(f, cam, inp.mouse),
            Mode::Scenario { editor } => editor.draw_world(f, cam),
        }
        // Top bar (game state text is drawn by the game's own UI on top of it).
        f.rect(0.0, 0.0, inp.screen[0], hud::TOP_H, style::PANEL);
        f.line([0.0, hud::TOP_H], [inp.screen[0], hud::TOP_H], 1.0, style::PANEL_EDGE);
        match &self.mode {
            Mode::Game { game, .. } => {
                game.draw_ui(f, inp.screen);
                if let Some(m) = &minimap {
                    f.rect(m.r.x - 2.0, m.r.y - 2.0, m.r.w + 4.0, m.r.h + 4.0, style::PANEL);
                    view.draw_minimap(f, m, cam);
                    game.draw_minimap(f, m);
                }
                if let (Some(p), Some(end)) = (self.end_panel(inp.screen), &game.ended) {
                    f.rect(p.x, p.y, p.w, p.h, style::PANEL);
                    f.rect_lines(p.x, p.y, p.w, p.h, style::AMBER);
                    f.text("MISSION END", p.x + 20.0, p.y + 12.0, style::FONT_S, style::TEXT_DIM);
                    let tw = f.text_width(end, style::FONT_L);
                    let size = if tw > p.w - 40.0 { style::FONT_S } else { style::FONT_L };
                    f.text(end, p.x + 20.0, p.y + 34.0, size, style::AMBER);
                }
            }
            Mode::Terrain { editor, .. } => {
                f.text(&self.title(), 10.0, 9.0, style::FONT_S, style::AMBER);
                editor.draw_ui(f, inp.screen);
            }
            Mode::Scenario { editor } => {
                f.text(&self.title(), 10.0, 9.0, style::FONT_S, style::AMBER);
                editor.draw_ui(f, inp.screen, ds);
            }
        }
        hud::draw(f, &game_buttons, inp.mouse);
        hud::draw(f, &screen_buttons, inp.mouse);
        status_line(f, inp, cam, map, ds, view.base);
        let menu = match &self.mode {
            Mode::Game { game, .. } => game.menu(),
            _ => None,
        };
        if let Some(m) = menu {
            m.draw(f, inp.screen, inp.mouse);
        } else {
            hud::tooltip(f, &screen_buttons, inp.mouse, inp.screen);
            hud::tooltip(f, &game_buttons, inp.mouse, inp.screen);
        }
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
    let right = format!("{}   {:.1} M/PX   F7 HELP   ESC MENU", base.label(), cam.mpp);
    let tw = f.text_width(&right, style::FONT_L - 8);
    f.text(&right, inp.screen[0] - tw - 10.0, y, style::FONT_L - 8, style::TEXT_DIM);
}

/// Keyboard / mouse reference over any screen.
pub fn help_overlay(f: &mut Frame, screen: P2, lines: &[&str]) {
    let w = lines.iter().map(|l| f.text_width(l, style::FONT_L - 8)).fold(600.0, f32::max) + 24.0;
    let h = lines.len() as f32 * 20.0 + 48.0;
    let (x, y) = ((screen[0] - w) / 2.0, (screen[1] - h) / 2.0);
    f.rect(x, y, w, h, style::PANEL);
    f.rect_lines(x, y, w, h, style::PANEL_EDGE);
    for (k, l) in lines.iter().enumerate() {
        f.text(l, x + 12.0, y + 12.0 + k as f32 * 20.0, style::FONT_L - 8, style::TEXT);
    }
    f.text("F7 OR ESC CLOSES", x + 12.0, y + h - 26.0, style::FONT_L - 8, style::TEXT_DIM);
}
