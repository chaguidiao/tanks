//! Terrain editor (TERRAIN.md §4), M0.5 basics: height brushes on the non-destructive edit
//! layer, draw river / road / forest / building features, delete features, undo/redo, save
//! and re-bake.
//!
//! Brushes preview live on the baked elevation; releasing the mouse re-bakes the whole map
//! (contours, carving, mobility) on a worker thread (`BakeQueue`), so editing never stalls.
//! Incremental per-tile baking comes later.

use std::sync::mpsc::{self, Receiver, TryRecvError};

use sim_data::TerrainData;
use terrain::features::{
    Building, BuildingClass, BuildingProps, Forest, ForestKind, ForestProps, River, RiverProps, Road, RoadClass,
    RoadProps,
};
use terrain::shade::CellRect;
use terrain::{AuthoringPackage, BakedMap, Features, Heightfield};

use crate::render::{Frame, Input, Key, P2};
use crate::style;
use crate::view::Camera;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Raise,
    Lower,
    Smooth,
    Flatten,
    River,
    Road,
    Forest,
    Building,
}

/// Default footprint width of a hand-placed building (m): a typical rural house.
const BUILDING_WIDTH_M: f64 = 9.0;

impl Tool {
    pub const ALL: [Tool; 8] =
        [Tool::Raise, Tool::Lower, Tool::Smooth, Tool::Flatten, Tool::River, Tool::Road, Tool::Forest, Tool::Building];

    pub fn label(self) -> &'static str {
        match self {
            Tool::Raise => "RAISE",
            Tool::Lower => "LOWER",
            Tool::Smooth => "SMOOTH",
            Tool::Flatten => "FLATTEN",
            Tool::River => "RIVER",
            Tool::Road => "ROAD",
            Tool::Forest => "FOREST",
            Tool::Building => "BUILDING",
        }
    }

    fn is_brush(self) -> bool {
        matches!(self, Tool::Raise | Tool::Lower | Tool::Smooth | Tool::Flatten)
    }
}

enum EditOp {
    Height { rect: CellRect, before: Vec<f32>, after: Vec<f32> },
    Features { before: Features, after: Features },
}

struct Stroke {
    snapshot: Vec<f32>,
    touched: Option<CellRect>,
    flatten_to: f64,
}

/// What the app must do after an editor update.
#[derive(Debug, Default)]
pub struct Effects {
    /// Cells changed by a brush this frame (refresh the preview texture).
    pub preview: Option<CellRect>,
    /// The authoring package changed: re-bake.
    pub rebake: bool,
    pub save: bool,
}

pub struct Editor {
    pub tool: Tool,
    pub radius_m: f64,
    /// Brush rate (m/s for raise/lower).
    pub strength: f64,
    pub pkg: AuthoringPackage,
    drawing: Vec<[f64; 2]>,
    stroke: Option<Stroke>,
    undo: Vec<EditOp>,
    redo: Vec<EditOp>,
    pub unsaved: bool,
    pub status: String,
}

fn union(a: Option<CellRect>, b: CellRect) -> CellRect {
    match a {
        None => b,
        Some(a) => CellRect { i0: a.i0.min(b.i0), j0: a.j0.min(b.j0), i1: a.i1.max(b.i1), j1: a.j1.max(b.j1) },
    }
}

fn crop(g: &[f32], w: usize, r: CellRect) -> Vec<f32> {
    let mut out = Vec::with_capacity(r.w() * r.h());
    for j in r.j0..r.j1 {
        out.extend_from_slice(&g[j * w + r.i0..j * w + r.i1]);
    }
    out
}

fn paste(g: &mut [f32], w: usize, r: CellRect, v: &[f32]) {
    for (row, j) in (r.j0..r.j1).enumerate() {
        g[j * w + r.i0..j * w + r.i1].copy_from_slice(&v[row * r.w()..(row + 1) * r.w()]);
    }
}

impl Editor {
    pub fn new(pkg: AuthoringPackage) -> Self {
        Self {
            tool: Tool::Raise,
            radius_m: 60.0,
            strength: 2.0,
            pkg,
            drawing: Vec::new(),
            stroke: None,
            undo: Vec::new(),
            redo: Vec::new(),
            unsaved: false,
            status: String::new(),
        }
    }

    pub fn update(&mut self, inp: &Input, cam: &Camera, baked: &mut BakedMap, over_ui: bool) -> Effects {
        let mut fx = Effects::default();
        for (k, t) in Tool::ALL.iter().enumerate() {
            if inp.hit(Key::Num(k as u8 + 1)) {
                self.tool = *t;
                self.drawing.clear();
            }
        }
        if inp.hit(Key::LeftBracket) {
            self.radius_m = (self.radius_m / 1.25).max(8.0);
        }
        if inp.hit(Key::RightBracket) {
            self.radius_m = (self.radius_m * 1.25).min(1000.0);
        }
        if inp.hit(Key::Minus) {
            self.strength = (self.strength / 1.5).max(0.1);
        }
        if inp.hit(Key::Equal) {
            self.strength = (self.strength * 1.5).min(50.0);
        }
        if inp.ctrl && inp.hit(Key::Char('z')) {
            fx.rebake |= self.undo_redo(true);
        }
        if inp.ctrl && inp.hit(Key::Char('y')) {
            fx.rebake |= self.undo_redo(false);
        }
        if inp.ctrl && inp.hit(Key::Char('s')) {
            fx.save = true;
        }
        if !inp.ctrl && inp.hit(Key::Char('b')) {
            fx.rebake = true;
        }
        if inp.hit(Key::Delete) && !over_ui {
            fx.rebake |= self.delete_feature(cam.to_world(inp.mouse), 12.0 * cam.mpp);
        }

        let world = cam.to_world(inp.mouse);
        if self.tool.is_brush() {
            if inp.left_pressed && !over_ui {
                let hf = self.pkg.edited_elevation();
                self.stroke = Some(Stroke {
                    snapshot: self.pkg.edit_height.data.clone(),
                    touched: None,
                    flatten_to: hf.height(world[0], world[1]),
                });
            }
            if inp.left_down
                && let Some(mut s) = self.stroke.take()
            {
                if let Some(r) = self.apply_brush(world, inp.frame_time as f64, s.flatten_to, baked) {
                    s.touched = Some(union(s.touched, r));
                    fx.preview = Some(r);
                }
                self.stroke = Some(s);
            }
            if inp.left_released
                && let Some(s) = self.stroke.take()
                && let Some(r) = s.touched
            {
                let w = self.pkg.edit_height.w;
                let before = crop(&s.snapshot, w, r);
                let after = crop(&self.pkg.edit_height.data, w, r);
                self.push(EditOp::Height { rect: r, before, after });
                fx.rebake = true;
            }
        } else {
            if inp.left_pressed && !over_ui {
                self.drawing.push(world);
                // Buildings take two clicks: centre, then the end of the long axis.
                if self.tool == Tool::Building && self.drawing.len() == 2 {
                    fx.rebake |= self.finish_feature();
                }
            }
            if inp.hit(Key::Backspace) {
                self.drawing.pop();
            }
            if inp.hit(Key::Escape) {
                self.drawing.clear();
            }
            if inp.hit(Key::Enter) {
                fx.rebake |= self.finish_feature();
            }
        }
        fx
    }

    fn push(&mut self, op: EditOp) {
        self.undo.push(op);
        self.redo.clear();
        self.unsaved = true;
        if self.undo.len() > 64 {
            self.undo.remove(0);
        }
    }

    fn undo_redo(&mut self, undo: bool) -> bool {
        let (from, to) = if undo { (&mut self.undo, &mut self.redo) } else { (&mut self.redo, &mut self.undo) };
        let Some(op) = from.pop() else { return false };
        let w = self.pkg.edit_height.w;
        match &op {
            EditOp::Height { rect, before, after } => {
                paste(&mut self.pkg.edit_height.data, w, *rect, if undo { before } else { after });
            }
            EditOp::Features { before, after } => {
                self.pkg.features = if undo { before.clone() } else { after.clone() };
            }
        }
        to.push(op);
        self.unsaved = true;
        self.status = if undo { "UNDO".into() } else { "REDO".into() };
        true
    }

    fn apply_brush(&mut self, at: [f64; 2], dt: f64, flatten_to: f64, baked: &mut BakedMap) -> Option<CellRect> {
        let cell = self.pkg.manifest.cell_size_m;
        let (w, h) = (self.pkg.edit_height.w, self.pkg.edit_height.h);
        let r = self.radius_m;
        let i0 = ((at[0] - r) / cell).floor().max(0.0) as usize;
        let j0 = ((at[1] - r) / cell).floor().max(0.0) as usize;
        let i1 = (((at[0] + r) / cell).ceil().max(0.0) as usize).min(w);
        let j1 = (((at[1] + r) / cell).ceil().max(0.0) as usize).min(h);
        if i0 >= i1 || j0 >= j1 {
            return None;
        }
        let rect = CellRect { i0, j0, i1, j1 };
        let base = &self.pkg.elevation_base;
        let edit = &self.pkg.edit_height;
        let z = |i: usize, j: usize| (base.get(i, j) + edit.get(i, j)) as f64;
        let mut deltas = Vec::with_capacity(rect.w() * rect.h());
        for j in j0..j1 {
            for i in i0..i1 {
                let (x, y) = ((i as f64 + 0.5) * cell, (j as f64 + 0.5) * cell);
                let d = ((x - at[0]).powi(2) + (y - at[1]).powi(2)).sqrt() / r;
                if d >= 1.0 {
                    deltas.push(0.0);
                    continue;
                }
                let fall = 1.0 - d * d * (3.0 - 2.0 * d); // smoothstep falloff
                let cur = z(i, j);
                let delta = match self.tool {
                    Tool::Raise => self.strength * dt * fall,
                    Tool::Lower => -self.strength * dt * fall,
                    Tool::Smooth => {
                        let mut s = 0.0;
                        let mut n = 0.0;
                        for dj in -1i64..=1 {
                            for di in -1i64..=1 {
                                let (a, b) = (i as i64 + di, j as i64 + dj);
                                if a >= 0 && b >= 0 && (a as usize) < w && (b as usize) < h {
                                    s += z(a as usize, b as usize);
                                    n += 1.0;
                                }
                            }
                        }
                        (s / n - cur) * (4.0 * dt * fall).min(1.0)
                    }
                    Tool::Flatten => (flatten_to - cur) * (3.0 * dt * fall).min(1.0),
                    _ => 0.0,
                };
                deltas.push(delta as f32);
            }
        }
        let mut k = 0;
        for j in j0..j1 {
            for i in i0..i1 {
                let d = deltas[k];
                k += 1;
                if d != 0.0 {
                    let idx = j * w + i;
                    self.pkg.edit_height.data[idx] += d;
                    baked.elevation.z.data[idx] += d;
                }
            }
        }
        // The hillshade of the ring just outside the brush changes too.
        Some(CellRect { i0: i0.saturating_sub(1), j0: j0.saturating_sub(1), i1: (i1 + 1).min(w), j1: (j1 + 1).min(h) })
    }

    /// Remove the feature nearest `at` (within `tol` m of its line, or inside its area).
    fn delete_feature(&mut self, at: [f64; 2], tol: f64) -> bool {
        let f = &self.pkg.features;
        let mut best: Option<(f64, u8, usize)> = None;
        let mut consider = |d: f64, kind: u8, k: usize| {
            if d <= tol && best.is_none_or(|(b, _, _)| d < b) {
                best = Some((d, kind, k));
            }
        };
        for (k, r) in f.rivers.iter().enumerate() {
            consider(polyline_dist(&r.points, at, false), 0, k);
        }
        for (k, r) in f.roads.iter().enumerate() {
            consider(polyline_dist(&r.points, at, false), 1, k);
        }
        for (k, b) in f.buildings.iter().enumerate() {
            consider(if inside(&b.ring, at) { 0.0 } else { polyline_dist(&b.ring, at, true) }, 2, k);
        }
        // Forests last and only by their edge or interior at a lower priority than small features.
        for (k, fo) in f.forests.iter().enumerate() {
            consider(if inside(&fo.ring, at) { tol } else { polyline_dist(&fo.ring, at, true) }, 3, k);
        }
        let Some((_, kind, k)) = best else {
            self.status = "NO FEATURE UNDER CURSOR".into();
            return false;
        };
        let before = self.pkg.features.clone();
        let f = &mut self.pkg.features;
        let what = match kind {
            0 => {
                f.rivers.remove(k);
                "RIVER"
            }
            1 => {
                f.roads.remove(k);
                "ROAD"
            }
            2 => {
                f.buildings.remove(k);
                "BUILDING"
            }
            _ => {
                f.forests.remove(k);
                "FOREST"
            }
        };
        let after = self.pkg.features.clone();
        self.push(EditOp::Features { before, after });
        self.status = format!("{what} DELETED");
        true
    }

    fn finish_feature(&mut self) -> bool {
        let min = if self.tool == Tool::Forest { 3 } else { 2 };
        if self.tool == Tool::Building {
            // Also accept ENTER after one click: default 12 m long, east-west.
            if self.drawing.len() == 1 {
                let c = self.drawing[0];
                self.drawing.push([c[0] + 6.0, c[1]]);
            }
        }
        if self.drawing.len() < min {
            self.status = format!("{}: NEED {min} POINTS", self.tool.label());
            return false;
        }
        let before = self.pkg.features.clone();
        let id = self.pkg.features.next_id();
        let pts = std::mem::take(&mut self.drawing);
        match self.tool {
            Tool::River => {
                // Order upstream → downstream: the baker keeps the water surface non-rising.
                let hf: Heightfield = self.pkg.edited_elevation();
                let mut pts = pts;
                let (a, b) = (pts[0], *pts.last().unwrap());
                if hf.height(a[0], a[1]) < hf.height(b[0], b[1]) {
                    pts.reverse();
                }
                let n = pts.len();
                self.pkg.features.rivers.push(River {
                    id,
                    points: pts,
                    props: RiverProps { width_m: vec![10.0; n], depth_m: vec![1.6; n] },
                });
            }
            Tool::Road => self.pkg.features.roads.push(Road {
                id,
                points: pts,
                props: RoadProps { class: RoadClass::Secondary, width_m: 5.0 },
            }),
            Tool::Building => {
                let (c, e) = (pts[0], pts[1]);
                let (dx, dy) = (e[0] - c[0], e[1] - c[1]);
                let half_l = (dx * dx + dy * dy).sqrt().max(4.0);
                let (ux, uy) = if half_l > 0.0 { (dx / half_l, dy / half_l) } else { (1.0, 0.0) };
                let (vx, vy) = (-uy * BUILDING_WIDTH_M / 2.0, ux * BUILDING_WIDTH_M / 2.0);
                let (ax, ay) = (ux * half_l, uy * half_l);
                let ring = vec![
                    [c[0] - ax - vx, c[1] - ay - vy],
                    [c[0] + ax - vx, c[1] + ay - vy],
                    [c[0] + ax + vx, c[1] + ay + vy],
                    [c[0] - ax + vx, c[1] - ay + vy],
                ];
                self.pkg.features.buildings.push(Building {
                    id,
                    ring,
                    props: BuildingProps { height_m: 7.0, class: BuildingClass::Masonry },
                });
            }
            Tool::Forest => self.pkg.features.forests.push(Forest {
                id,
                ring: pts,
                props: ForestProps { kind: ForestKind::Deciduous, height_m: 18.0 },
            }),
            _ => return false,
        }
        let after = self.pkg.features.clone();
        self.push(EditOp::Features { before, after });
        self.status = format!("{} ADDED", self.tool.label());
        true
    }

    pub fn draw_world(&self, f: &mut Frame, cam: &Camera, mouse: P2) {
        if self.tool.is_brush() {
            let r = (self.radius_m / cam.mpp) as f32;
            f.circle_lines(mouse, r, style::EDIT_CURSOR);
            f.circle_lines(mouse, 2.0, style::EDIT_CURSOR);
        } else if !self.drawing.is_empty() {
            let mut pts: Vec<P2> = self.drawing.iter().map(|&p| cam.to_screen(p)).collect();
            pts.push(mouse);
            if self.tool == Tool::Forest {
                pts.push(pts[0]);
            }
            for w in pts.windows(2) {
                f.line(w[0], w[1], 1.5, style::EDIT_CURSOR);
            }
            for p in &pts {
                f.circle(*p, 3.0, style::EDIT_CURSOR);
            }
        }
    }

    pub fn draw_ui(&self, f: &mut Frame, screen: P2) {
        let (x, y, w, h) = (10.0, 40.0, 190.0, 248.0);
        f.rect(x, y, w, h, style::PANEL);
        f.rect_lines(x, y, w, h, style::PANEL_EDGE);
        f.text("TOOLS", x + 10.0, y + 8.0, style::FONT_L - 6, style::AMBER);
        for (k, t) in Tool::ALL.iter().enumerate() {
            let c = if *t == self.tool { style::SELECT } else { style::TEXT };
            let mark = if *t == self.tool { ">" } else { " " };
            f.text(
                &format!("{mark}{} {}", k + 1, t.label()),
                x + 10.0,
                y + 32.0 + k as f32 * 18.0,
                style::FONT_L - 8,
                c,
            );
        }
        let info = [
            format!("RADIUS  {:.0} M   [ ]", self.radius_m),
            format!("RATE    {:.1} M/S - =", self.strength),
            if self.unsaved { "UNSAVED  CTRL+S".to_owned() } else { "SAVED".to_owned() },
        ];
        for (k, l) in info.iter().enumerate() {
            f.text(l, x + 10.0, y + 184.0 + k as f32 * 18.0, style::FONT_L - 8, style::TEXT_DIM);
        }
        let hint = if self.tool.is_brush() {
            "LMB PAINT  DEL DELETE FEATURE  CTRL+Z/Y UNDO/REDO  B BAKE  CTRL+S SAVE  ESC MENU"
        } else if self.tool == Tool::Building {
            "LMB CENTRE, LMB END OF LONG AXIS  DEL DELETE FEATURE  ESC CANCEL"
        } else {
            "LMB ADD POINT  ENTER FINISH  BKSP UNDO POINT  ESC CANCEL"
        };
        f.text(hint, 10.0, screen[1] - 44.0, style::FONT_L - 8, style::TEXT_DIM);
        f.text(&self.status, 220.0, 44.0, style::FONT_L - 8, style::AMBER);
    }

    /// A feature is being drawn (Esc cancels it rather than opening the menu).
    pub fn busy(&self) -> bool {
        !self.drawing.is_empty() || self.stroke.is_some()
    }

    pub fn over_ui(&self, s: P2) -> bool {
        s[0] < 200.0 && s[1] > 40.0 && s[1] < 288.0
    }
}

fn seg_dist(a: [f64; 2], b: [f64; 2], p: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    ((a[0] + t * dx - p[0]).powi(2) + (a[1] + t * dy - p[1]).powi(2)).sqrt()
}

fn polyline_dist(pts: &[[f64; 2]], p: [f64; 2], closed: bool) -> f64 {
    let mut d = f64::INFINITY;
    for w in pts.windows(2) {
        d = d.min(seg_dist(w[0], w[1], p));
    }
    if closed && pts.len() > 2 {
        d = d.min(seg_dist(pts[pts.len() - 1], pts[0], p));
    }
    d
}

/// Even-odd point-in-polygon test.
fn inside(ring: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut c = false;
    let n = ring.len();
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + n - 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
            c = !c;
        }
    }
    c
}

/// Background re-bake: at most one job runs; edits made meanwhile make its result stale, and
/// a stale result is dropped and the job restarted from the current package (latest wins).
pub struct BakeQueue<T> {
    /// Edit generation: bumped by every brush preview and package change.
    edit: u64,
    /// A package change still needs a bake.
    wanted: bool,
    running: Option<Receiver<(u64, T)>>,
}

impl<T> Default for BakeQueue<T> {
    fn default() -> Self {
        Self { edit: 0, wanted: false, running: None }
    }
}

#[derive(Debug)]
pub enum BakeEvent<T> {
    Idle,
    Started,
    Done(T),
    Failed,
}

impl<T: Send + 'static> BakeQueue<T> {
    /// The baked map was changed by a live preview (its bake result would undo that).
    pub fn touch(&mut self) {
        self.edit += 1;
    }

    /// The authoring package changed: bake it.
    pub fn request(&mut self) {
        self.edit += 1;
        self.wanted = true;
    }

    /// A bake is wanted or running: the current baked map may be out of date.
    #[cfg(test)]
    pub fn pending(&self) -> bool {
        self.wanted || self.running.is_some()
    }

    /// Forget wanted and running bakes (the caller bakes synchronously instead).
    pub fn cancel(&mut self) {
        self.wanted = false;
        self.running = None;
    }

    /// Collect a finished bake and start a wanted one. `job` is called only when a bake
    /// starts and returns the work to run on the worker thread.
    pub fn poll<J>(&mut self, job: impl FnOnce() -> J) -> BakeEvent<T>
    where
        J: FnOnce() -> T + Send + 'static,
    {
        let mut ev = BakeEvent::Idle;
        if let Some(rx) = &self.running {
            match rx.try_recv() {
                Ok((edit, out)) => {
                    self.running = None;
                    if edit == self.edit {
                        ev = BakeEvent::Done(out);
                    } else {
                        self.wanted = true;
                    }
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    self.running = None;
                    ev = BakeEvent::Failed;
                }
            }
        }
        if self.running.is_none() && self.wanted {
            self.wanted = false;
            let (tx, rx) = mpsc::channel();
            let (edit, work) = (self.edit, job());
            std::thread::spawn(move || {
                let _ = tx.send((edit, work()));
            });
            self.running = Some(rx);
            if matches!(ev, BakeEvent::Idle) {
                ev = BakeEvent::Started;
            }
        }
        ev
    }
}

/// Re-bake helper used by the app (kept here so the editor owns its package).
pub fn rebake(pkg: &AuthoringPackage, td: &TerrainData, classes: &[terrain::mobility::MobilityClass]) -> BakedMap {
    terrain::bake(pkg, td, classes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use terrain::MapManifest;

    fn setup() -> (Editor, BakedMap, Camera, sim_data::DataSet) {
        let ds = sim_data::DataSet::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")).unwrap();
        let m = MapManifest {
            id: "t".into(),
            name: "t".into(),
            width_cells: 100,
            height_cells: 100,
            cell_size_m: 4.0,
            seed: 1,
            recipe: None,
        };
        let pkg = AuthoringPackage::new_flat(m, 50.0);
        let baked = rebake(&pkg, &ds.terrain, &[]);
        let cam = Camera { centre: [200.0, 200.0], mpp: 1.0, screen: [400.0, 400.0] };
        (Editor::new(pkg), baked, cam, ds)
    }

    fn input(mouse: P2) -> Input {
        Input { mouse, frame_time: 0.5, screen: [400.0, 400.0], ..Default::default() }
    }

    fn wait<T: Send + 'static>(q: &mut BakeQueue<T>, job: impl Fn() -> T + Send + Clone + 'static) -> BakeEvent<T> {
        for _ in 0..500 {
            let j = job.clone();
            match q.poll(move || j) {
                BakeEvent::Idle => std::thread::sleep(std::time::Duration::from_millis(5)),
                e => return e,
            }
        }
        panic!("bake did not finish");
    }

    #[test]
    fn bake_queue_delivers_and_drops_stale_results() {
        let mut q = BakeQueue::<u32>::default();
        assert!(matches!(q.poll(|| || 1), BakeEvent::Idle));
        q.request();
        assert!(matches!(q.poll(|| || 1), BakeEvent::Started));
        assert!(q.pending());
        assert!(matches!(wait(&mut q, || 1), BakeEvent::Done(1)));
        assert!(!q.pending());
        // An edit while the job runs makes its result stale: it restarts instead.
        q.request();
        assert!(matches!(q.poll(|| || 2), BakeEvent::Started));
        q.touch();
        assert!(matches!(wait(&mut q, || 3), BakeEvent::Started));
        assert!(matches!(wait(&mut q, || 3), BakeEvent::Done(3)));
    }

    #[test]
    fn background_bake_matches_a_synchronous_bake() {
        let (mut ed, mut baked, cam, ds) = setup();
        let mut i = input([200.0, 200.0]);
        i.left_pressed = true;
        i.left_down = true;
        ed.update(&i, &cam, &mut baked, false);
        let mut i = input([200.0, 200.0]);
        i.left_released = true;
        assert!(ed.update(&i, &cam, &mut baked, false).rebake);
        let td = std::sync::Arc::new(ds.terrain.clone());
        let mut q = BakeQueue::default();
        q.request();
        let job = {
            let (pkg, td) = (ed.pkg.clone(), td.clone());
            move || rebake(&pkg, &td, &[]).output_hash()
        };
        let first = q.poll(|| job.clone());
        assert!(matches!(first, BakeEvent::Started));
        let BakeEvent::Done(h) = wait(&mut q, job) else { panic!("no result") };
        assert_eq!(h, rebake(&ed.pkg, &ds.terrain, &[]).output_hash());
    }

    #[test]
    fn raise_brush_edits_layer_and_undo_restores() {
        let (mut ed, mut baked, cam, _) = setup();
        let centre = [200.0, 200.0]; // screen centre = world (200, 200)
        let mut i = input(centre);
        i.left_pressed = true;
        i.left_down = true;
        let fx = ed.update(&i, &cam, &mut baked, false);
        assert!(fx.preview.is_some());
        let mut i = input(centre);
        i.left_released = true;
        let fx = ed.update(&i, &cam, &mut baked, false);
        assert!(fx.rebake);
        let k = 50 * 100 + 50;
        assert!(ed.pkg.edit_height.data[k] > 0.5, "raised by rate × dt at the centre");
        assert_eq!(ed.pkg.elevation_base.data[k], 50.0, "base is never modified");
        let mut i = input(centre);
        i.ctrl = true;
        i.pressed.push(Key::Char('z'));
        assert!(ed.update(&i, &cam, &mut baked, false).rebake);
        assert!(ed.pkg.edit_height.data.iter().all(|&v| v == 0.0), "undo restores the edit layer");
    }

    #[test]
    fn drawing_a_road_adds_a_feature() {
        let (mut ed, mut baked, cam, ds) = setup();
        let mut i = input([0.0, 0.0]);
        i.pressed.push(Key::Num(6));
        ed.update(&i, &cam, &mut baked, false);
        assert_eq!(ed.tool, Tool::Road);
        for p in [[20.0, 200.0], [380.0, 210.0]] {
            let mut i = input(p);
            i.left_pressed = true;
            ed.update(&i, &cam, &mut baked, false);
        }
        let mut i = input([0.0, 0.0]);
        i.pressed.push(Key::Enter);
        assert!(ed.update(&i, &cam, &mut baked, false).rebake);
        assert_eq!(ed.pkg.features.roads.len(), 1);
        let b = rebake(&ed.pkg, &ds.terrain, &[]);
        let road = ds.terrain.landcover_id("road").unwrap();
        assert!(b.landcover.data.contains(&road), "road rasterised by the baker");
    }
}
