//! Camera and the tactical map display: base raster layers, contours, grid, vector features.

use sim_data::TerrainData;
use terrain::BakedMap;
use terrain::contour::ContourKind;
use terrain::features::RoadClass;
use terrain::shade::{self, CellRect};

use crate::hud::Rect;
use crate::render::{Frame, Gfx, P2, Rgba, Tex};
use crate::style;

#[derive(Debug, Clone, Copy)]
pub struct Camera {
    pub centre: [f64; 2],
    /// Metres per screen pixel.
    pub mpp: f64,
    pub screen: P2,
}

impl Camera {
    pub fn to_screen(self, p: [f64; 2]) -> P2 {
        [
            ((p[0] - self.centre[0]) / self.mpp) as f32 + self.screen[0] / 2.0,
            self.screen[1] / 2.0 - ((p[1] - self.centre[1]) / self.mpp) as f32,
        ]
    }

    pub fn to_world(self, s: P2) -> [f64; 2] {
        [
            self.centre[0] + (s[0] - self.screen[0] / 2.0) as f64 * self.mpp,
            self.centre[1] - (s[1] - self.screen[1] / 2.0) as f64 * self.mpp,
        ]
    }

    /// Zoom by `factor` keeping the world point under `s` fixed.
    pub fn zoom_at(&mut self, s: P2, factor: f64) {
        let before = self.to_world(s);
        self.mpp = (self.mpp * factor).clamp(0.05, 40.0);
        let after = self.to_world(s);
        self.centre[0] += before[0] - after[0];
        self.centre[1] += before[1] - after[1];
    }

    pub fn pan_px(&mut self, d: P2) {
        self.centre[0] -= d[0] as f64 * self.mpp;
        self.centre[1] += d[1] as f64 * self.mpp;
    }

    /// Visible world rectangle (xmin, ymin, xmax, ymax).
    pub fn view(self) -> [f64; 4] {
        let a = self.to_world([0.0, self.screen[1]]);
        let b = self.to_world([self.screen[0], 0.0]);
        [a[0], a[1], b[0], b[1]]
    }

    pub fn fit(&mut self, w_m: f64, h_m: f64) {
        self.centre = [w_m / 2.0, h_m / 2.0];
        self.mpp = (w_m / self.screen[0] as f64).max(h_m / self.screen[1] as f64) * 1.05;
    }
}

/// Overview map: the whole terrain fitted (aspect kept) into a screen frame.
#[derive(Debug, Clone, Copy)]
pub struct Minimap {
    /// Image rectangle on screen (inside the frame).
    pub r: Rect,
    pub w_m: f64,
    pub h_m: f64,
}

impl Minimap {
    pub fn new(frame: Rect, w_m: f64, h_m: f64) -> Self {
        let s = (frame.w as f64 / w_m).min(frame.h as f64 / h_m);
        let (w, h) = ((w_m * s) as f32, (h_m * s) as f32);
        Self { r: Rect::new(frame.x + (frame.w - w) / 2.0, frame.y + (frame.h - h) / 2.0, w, h), w_m, h_m }
    }

    pub fn to_px(self, p: [f64; 2]) -> P2 {
        [self.r.x + (p[0] / self.w_m) as f32 * self.r.w, self.r.y + self.r.h - (p[1] / self.h_m) as f32 * self.r.h]
    }

    pub fn to_world(self, s: P2) -> [f64; 2] {
        [((s[0] - self.r.x) / self.r.w) as f64 * self.w_m, ((self.r.y + self.r.h - s[1]) / self.r.h) as f64 * self.h_m]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaseLayer {
    Tactical,
    Slope,
    Trafficability,
    LandCover,
}

impl BaseLayer {
    pub fn label(self) -> &'static str {
        match self {
            BaseLayer::Tactical => "RELIEF",
            BaseLayer::Slope => "SLOPE",
            BaseLayer::Trafficability => "GO/SLOW-GO/NO-GO",
            BaseLayer::LandCover => "LAND COVER",
        }
    }
}

pub struct MapView {
    pub base: BaseLayer,
    pub contours: bool,
    pub grid: bool,
    pub features: bool,
    /// Vehicle class shown by the trafficability layer.
    pub traffic_class: String,
    textures: [Option<Tex>; 4],
    /// Analysis overlay (viewshed) with its world rectangle (xmin, ymin, xmax, ymax).
    pub overlay: Option<(Tex, [f64; 4])>,
    contour_bbox: Vec<[f32; 4]>,
    range: (f32, f32),
}

fn slot(b: BaseLayer) -> usize {
    b as usize
}

/// RGBA image of a base layer (pure CPU work, safe on a worker thread).
pub fn build_layer(base: BaseLayer, traffic_class: &str, baked: &BakedMap, td: &TerrainData) -> Vec<u8> {
    let hf = &baked.elevation;
    match base {
        BaseLayer::Tactical => {
            shade::tactical(hf, &baked.landcover, &baked.water_depth, td, hf.min_max(), CellRect::full(hf.z.w, hf.z.h))
        }
        BaseLayer::Slope => shade::slope_view(hf),
        BaseLayer::LandCover => shade::landcover_view(&baked.landcover, td),
        BaseLayer::Trafficability => match baked.mobility(traffic_class).or(baked.mobility.first()) {
            Some(m) => {
                let r = (m.class.cross_country_speed / terrain::mobility::SPEED_QUANTUM) as u8;
                shade::trafficability_view(&m.speed, r)
            }
            None => shade::slope_view(hf),
        },
    }
}

impl MapView {
    pub fn new(baked: &BakedMap, traffic_class: &str) -> Self {
        let mut v = Self {
            base: BaseLayer::Tactical,
            contours: true,
            grid: true,
            features: true,
            traffic_class: traffic_class.to_owned(),
            textures: [None, None, None, None],
            overlay: None,
            contour_bbox: Vec::new(),
            range: (0.0, 1.0),
        };
        v.invalidate(baked);
        v
    }

    /// Call after the baked map changed: textures are rebuilt lazily.
    pub fn invalidate(&mut self, baked: &BakedMap) {
        self.textures = [None, None, None, None];
        self.range = baked.elevation.min_max();
        self.contour_bbox = baked
            .contours
            .iter()
            .map(|c| {
                c.points.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |b, p| {
                    [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])]
                })
            })
            .collect();
    }

    fn build(&self, baked: &BakedMap, td: &TerrainData) -> Vec<u8> {
        build_layer(self.base, &self.traffic_class, baked, td)
    }

    /// Swap in a new baked map whose `base` layer image was built off the UI thread
    /// (`build_layer`): the existing texture is updated in place, other layers rebuild lazily.
    pub fn replace_map(&mut self, gfx: &mut Gfx, baked: &BakedMap, base: BaseLayer, img: &[u8]) {
        let s = slot(base);
        let keep = self.textures[s].take().filter(|t| t.w == baked.elevation.z.w && t.h == baked.elevation.z.h);
        self.invalidate(baked);
        self.textures[s] = Some(match keep {
            Some(mut t) => {
                gfx.update(&mut t, img);
                t
            }
            None => gfx.texture(baked.elevation.z.w, baked.elevation.z.h, img),
        });
    }

    pub fn ensure(&mut self, gfx: &mut Gfx, baked: &BakedMap, td: &TerrainData) {
        let s = slot(self.base);
        if self.textures[s].is_none() {
            let img = self.build(baked, td);
            self.textures[s] = Some(gfx.texture(baked.elevation.z.w, baked.elevation.z.h, &img));
        }
    }

    /// Live preview of an edited cell rectangle on the tactical layer (other layers are dropped).
    pub fn refresh_rect(&mut self, gfx: &mut Gfx, baked: &BakedMap, td: &TerrainData, r: CellRect) {
        for (k, t) in self.textures.iter_mut().enumerate() {
            if k != slot(BaseLayer::Tactical) {
                *t = None;
            }
        }
        if let Some(t) = &mut self.textures[slot(BaseLayer::Tactical)] {
            let img = shade::tactical(&baked.elevation, &baked.landcover, &baked.water_depth, td, self.range, r);
            let h = baked.elevation.z.h;
            gfx.update_rect(t, r.i0, h - r.j1, r.w(), r.h(), &img);
        }
    }

    pub fn draw(&self, f: &mut Frame, cam: &Camera, baked: &BakedMap) {
        let (w_m, h_m) = (baked.manifest.width_m(), baked.manifest.height_m());
        if let Some(t) = &self.textures[slot(self.base)] {
            let a = cam.to_screen([0.0, h_m]);
            let b = cam.to_screen([w_m, 0.0]);
            f.texture(t, a[0], a[1], b[0] - a[0], b[1] - a[1], crate::render::Rgba(255, 255, 255, 255));
        }
        if let Some((t, r)) = &self.overlay {
            let a = cam.to_screen([r[0], r[3]]);
            let b = cam.to_screen([r[2], r[1]]);
            f.texture(t, a[0], a[1], b[0] - a[0], b[1] - a[1], crate::render::Rgba(255, 255, 255, 255));
        }
        if self.features {
            draw_features(f, cam, baked);
        }
        if self.contours {
            self.draw_contours(f, cam, baked);
        }
        if self.grid {
            draw_grid(f, cam, w_m, h_m);
        }
    }

    /// The current base layer, whole map, into the minimap; plus the camera's view rectangle.
    pub fn draw_minimap(&self, f: &mut Frame, m: &Minimap, cam: &Camera) {
        if let Some(t) = &self.textures[slot(self.base)] {
            f.texture(t, m.r.x, m.r.y, m.r.w, m.r.h, Rgba(200, 200, 200, 255));
        }
        f.rect_lines(m.r.x, m.r.y, m.r.w, m.r.h, style::PANEL_EDGE);
        let v = cam.view();
        let a = m.to_px([v[0].max(0.0), v[3].min(m.h_m)]);
        let b = m.to_px([v[2].min(m.w_m), v[1].max(0.0)]);
        if b[0] > a[0] && b[1] > a[1] {
            f.rect_lines(a[0], a[1], b[0] - a[0], b[1] - a[1], style::SELECT);
        }
    }

    fn draw_contours(&self, f: &mut Frame, cam: &Camera, baked: &BakedMap) {
        let v = cam.view();
        let show_inter = cam.mpp < 10.0;
        let show_supp = cam.mpp < 1.6;
        let label = cam.mpp < 3.0;
        for (c, bb) in baked.contours.iter().zip(&self.contour_bbox) {
            let (color, width) = match c.kind {
                ContourKind::Index => (style::CONTOUR_INDEX, 1.6),
                ContourKind::Intermediate if show_inter => (style::CONTOUR, 1.0),
                ContourKind::Supplementary if show_supp => (style::CONTOUR_SUPP, 1.0),
                _ => continue,
            };
            if (bb[2] as f64) < v[0] || (bb[0] as f64) > v[2] || (bb[3] as f64) < v[1] || (bb[1] as f64) > v[3] {
                continue;
            }
            let mut prev = cam.to_screen([c.points[0][0] as f64, c.points[0][1] as f64]);
            for p in c.points.iter().skip(1) {
                let s = cam.to_screen([p[0] as f64, p[1] as f64]);
                f.line(prev, s, width, color);
                prev = s;
            }
            if c.closed {
                let s = cam.to_screen([c.points[0][0] as f64, c.points[0][1] as f64]);
                f.line(prev, s, width, color);
            }
            if label && c.kind == ContourKind::Index && c.points.len() > 2 {
                let p = c.points[c.points.len() / 2];
                let s = cam.to_screen([p[0] as f64, p[1] as f64]);
                f.text(&format!("{:.0}", c.level), s[0] + 2.0, s[1] - 5.0, style::FONT, style::CONTOUR_INDEX);
            }
        }
    }
}

fn polyline(f: &mut Frame, cam: &Camera, pts: &[[f64; 2]], width: f32, c: crate::render::Rgba) {
    for w in pts.windows(2) {
        f.line(cam.to_screen(w[0]), cam.to_screen(w[1]), width, c);
    }
}

fn draw_buildings(f: &mut Frame, cam: &Camera, baked: &BakedMap) {
    if cam.mpp > 4.0 {
        return;
    }
    let v = cam.view();
    for b in &baked.features.buildings {
        let Some(p0) = b.ring.first() else { continue };
        if p0[0] < v[0] - 30.0 || p0[0] > v[2] + 30.0 || p0[1] < v[1] - 30.0 || p0[1] > v[3] + 30.0 {
            continue;
        }
        let pts: Vec<P2> = b.ring.iter().map(|&p| cam.to_screen(p)).collect();
        f.poly(&pts, style::BUILDING);
        for k in 0..pts.len() {
            f.line(pts[k], pts[(k + 1) % pts.len()], 1.0, style::BUILDING_EDGE);
        }
    }
}

fn draw_features(f: &mut Frame, cam: &Camera, baked: &BakedMap) {
    draw_buildings(f, cam, baked);
    for r in &baked.features.rivers {
        let w = r.props.width_m.iter().copied().fold(0.0f32, f32::max) as f64;
        polyline(f, cam, &r.points, ((w / cam.mpp) as f32).max(1.2), style::WATER);
    }
    for f_ in &baked.features.forests {
        let mut ring = f_.ring.clone();
        if let Some(&p) = ring.first() {
            ring.push(p);
        }
        polyline(f, cam, &ring, 1.0, style::FOREST_EDGE);
    }
    for r in &baked.features.roads {
        let w_px = (r.props.width_m as f64 / cam.mpp) as f32;
        match r.props.class {
            RoadClass::Main => {
                // Thin double line: light casing, dark fill.
                polyline(f, cam, &r.points, (w_px + 2.0).max(3.0), style::ROAD);
                polyline(f, cam, &r.points, w_px.max(1.0), style::ROAD_FILL);
            }
            RoadClass::Secondary => polyline(f, cam, &r.points, w_px.max(1.5), style::ROAD),
            RoadClass::Street => polyline(f, cam, &r.points, w_px.max(1.0), style::STREET),
            RoadClass::Track => {
                // Dashed.
                for (k, w) in r.points.windows(2).enumerate() {
                    if k % 2 == 0 {
                        f.line(cam.to_screen(w[0]), cam.to_screen(w[1]), w_px.max(1.0), style::TRACK);
                    }
                }
            }
        }
    }
}

fn draw_grid(f: &mut Frame, cam: &Camera, w_m: f64, h_m: f64) {
    let step = if cam.mpp > 12.0 { 2000.0 } else { 1000.0 };
    let v = cam.view();
    let mut x = (v[0].max(0.0) / step).floor() * step;
    while x <= v[2].min(w_m) {
        let a = cam.to_screen([x, 0.0f64.max(v[1])]);
        let b = cam.to_screen([x, h_m.min(v[3])]);
        f.line(a, b, 1.0, style::GRID);
        f.text(
            &format!("{:02}", (x / 1000.0) as i64),
            a[0] + 2.0,
            crate::hud::TOP_H + 4.0,
            style::FONT,
            style::GRID_LABEL,
        );
        x += step;
    }
    let mut y = (v[1].max(0.0) / step).floor() * step;
    while y <= v[3].min(h_m) {
        let a = cam.to_screen([0.0f64.max(v[0]), y]);
        let b = cam.to_screen([w_m.min(v[2]), y]);
        f.line(a, b, 1.0, style::GRID);
        f.text(&format!("{:02}", (y / 1000.0) as i64), 4.0, a[1] - 11.0, style::FONT, style::GRID_LABEL);
        y += step;
    }
}
