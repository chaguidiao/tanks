//! NATO symbology subset (MIL-STD-2525D / APP-6D) drawn from primitives: frame shape encodes
//! affiliation (rectangle friendly, diamond hostile), the ellipse is the armour icon and the
//! crossed diagonals the infantry icon.

use crate::render::{Frame, P2, Rgba};
use crate::style;
use crate::view::Camera;

const W: f32 = 26.0;
const H: f32 = 17.0;

pub fn friendly_armour(f: &mut Frame, p: P2, c: Rgba) {
    f.rect(p[0] - W / 2.0, p[1] - H / 2.0, W, H, Rgba(10, 18, 24, 200));
    f.rect_lines(p[0] - W / 2.0, p[1] - H / 2.0, W, H, c);
    f.rect_lines(p[0] - W / 2.0 + 1.0, p[1] - H / 2.0 + 1.0, W - 2.0, H - 2.0, c);
    f.ellipse_lines(p, W * 0.32, H * 0.26, c);
}

pub fn hostile_armour(f: &mut Frame, p: P2, c: Rgba) {
    let r = H * 0.9;
    let pts = [[p[0], p[1] - r], [p[0] + r, p[1]], [p[0], p[1] + r], [p[0] - r, p[1]]];
    for k in 0..4 {
        f.line(pts[k], pts[(k + 1) % 4], 2.0, c);
    }
    f.ellipse_lines(p, r * 0.45, r * 0.28, c);
}

/// Friendly infantry (APP-6D: rectangle with crossed diagonals), drawn at `scale` of the armour
/// frame (squads are smaller than vehicles on the map).
pub fn friendly_infantry(f: &mut Frame, p: P2, c: Rgba, scale: f32) {
    let (w, h) = (W * scale, H * scale);
    let (x0, y0) = (p[0] - w / 2.0, p[1] - h / 2.0);
    f.rect(x0, y0, w, h, Rgba(10, 18, 24, 200));
    f.rect_lines(x0, y0, w, h, c);
    f.line([x0, y0], [x0 + w, y0 + h], 1.5, c);
    f.line([x0, y0 + h], [x0 + w, y0], 1.5, c);
}

/// Hostile infantry: diamond with crossed diagonals, at `scale`.
pub fn hostile_infantry(f: &mut Frame, p: P2, c: Rgba, scale: f32) {
    let r = H * 0.9 * scale;
    let pts = [[p[0], p[1] - r], [p[0] + r, p[1]], [p[0], p[1] + r], [p[0] - r, p[1]]];
    for k in 0..4 {
        f.line(pts[k], pts[(k + 1) % 4], 2.0, c);
    }
    let d = r * 0.5;
    f.line([p[0] - d, p[1] - d], [p[0] + d, p[1] + d], 1.5, c);
    f.line([p[0] - d, p[1] + d], [p[0] + d, p[1] - d], 1.5, c);
}

/// Anti-armour modifier (APP-6D: an inverted V inside the infantry frame) for anti-tank missile
/// teams and squads seen launching, at `scale`; `friendly` picks the rectangle's or the diamond's
/// proportions.
pub fn anti_armour(f: &mut Frame, p: P2, c: Rgba, scale: f32, friendly: bool) {
    let (dx, dy) = if friendly { (W * scale / 2.0, H * scale / 2.0) } else { (H * 0.45 * scale, H * 0.45 * scale) };
    f.line([p[0] - dx, p[1] + dy], [p[0], p[1] - dy], 1.5, c);
    f.line([p[0], p[1] - dy], [p[0] + dx, p[1] + dy], 1.5, c);
}

/// Squad echelon amplifier: one dot above a frame of half-height `h2`.
pub fn echelon_squad(f: &mut Frame, p: P2, h2: f32, c: Rgba) {
    f.circle([p[0], p[1] - h2 - 4.0], 1.6, c);
}

/// Squads are drawn at this fraction of a vehicle symbol.
pub const SQUAD_SCALE: f32 = 0.75;

/// Field artillery: friendly rectangle or hostile diamond frame with a filled dot (APP-6D).
pub fn field_artillery(f: &mut Frame, p: P2, c: Rgba, friendly: bool) {
    if friendly {
        f.rect(p[0] - W / 2.0, p[1] - H / 2.0, W, H, Rgba(10, 18, 24, 200));
        f.rect_lines(p[0] - W / 2.0, p[1] - H / 2.0, W, H, c);
        f.rect_lines(p[0] - W / 2.0 + 1.0, p[1] - H / 2.0 + 1.0, W - 2.0, H - 2.0, c);
    } else {
        let r = H * 0.9;
        let pts = [[p[0], p[1] - r], [p[0] + r, p[1]], [p[0], p[1] + r], [p[0] - r, p[1]]];
        for k in 0..4 {
            f.line(pts[k], pts[(k + 1) % 4], 2.0, c);
        }
    }
    f.circle(p, 3.5, c);
}

/// Direction-of-movement staff from the symbol centre.
pub fn heading_staff(f: &mut Frame, p: P2, heading: f64, len: f32, c: Rgba) {
    let (s, co) = (heading.sin() as f32, heading.cos() as f32);
    let a = [p[0] + s * (H / 2.0), p[1] - co * (H / 2.0)];
    let b = [p[0] + s * (H / 2.0 + len), p[1] - co * (H / 2.0 + len)];
    f.line(a, b, 1.5, c);
}

/// Unknown affiliation: quatrefoil-like frame (circle with four lobes).
pub fn unknown_frame(f: &mut Frame, p: P2, c: Rgba) {
    f.circle_lines(p, H * 0.6, c);
    for (dx, dy) in [(0.0, -1.0), (1.0, 0.0), (0.0, 1.0), (-1.0, 0.0)] {
        f.circle_lines([p[0] + dx * H * 0.55, p[1] + dy * H * 0.55], H * 0.32, c);
    }
}

/// Destroyed: a cross over the symbol.
pub fn destroyed(f: &mut Frame, p: P2, c: Rgba) {
    let r = H * 0.9;
    f.line([p[0] - r, p[1] - r], [p[0] + r, p[1] + r], 2.0, c);
    f.line([p[0] - r, p[1] + r], [p[0] + r, p[1] - r], 2.0, c);
}

/// Damaged (mobility or firepower kill): a single slash.
pub fn damaged(f: &mut Frame, p: P2, c: Rgba) {
    let r = H * 0.9;
    f.line([p[0] - r, p[1] + r], [p[0] + r, p[1] - r], 2.0, c);
}

/// Platoon echelon amplifier: three dots above the frame.
pub fn echelon_platoon(f: &mut Frame, p: P2, c: Rgba) {
    for k in -1..=1 {
        f.circle([p[0] + k as f32 * 5.0, p[1] - H / 2.0 - 5.0], 1.6, c);
    }
}

fn dashed(f: &mut Frame, a: P2, b: P2, c: Rgba) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = (dx * dx + dy * dy).sqrt();
    let n = (len / 6.0).max(1.0) as usize;
    for k in (0..n).step_by(2) {
        let (t0, t1) = (k as f32 / n as f32, ((k + 1) as f32 / n as f32).min(1.0));
        f.line([a[0] + dx * t0, a[1] + dy * t0], [a[0] + dx * t1, a[1] + dy * t1], 1.5, c);
    }
}

/// Battle position graphic: circle, orientation arrow towards the threat, label.
/// Dashed while planned (not yet acknowledged), solid once executing.
pub fn battle_position(
    f: &mut Frame,
    cam: &Camera,
    centre: [f64; 2],
    radius: f64,
    facing: f64,
    label: &str,
    solid: bool,
) {
    let c = style::FRIENDLY;
    let n = 48;
    let pts: Vec<P2> = (0..=n)
        .map(|k| {
            let a = k as f64 / n as f64 * std::f64::consts::TAU;
            cam.to_screen([centre[0] + radius * a.sin(), centre[1] + radius * a.cos()])
        })
        .collect();
    for w in pts.windows(2) {
        if solid {
            f.line(w[0], w[1], 1.5, c);
        } else {
            dashed(f, w[0], w[1], c);
        }
    }
    let tip = [centre[0] + facing.sin() * radius * 1.6, centre[1] + facing.cos() * radius * 1.6];
    let (a, b) = (cam.to_screen(centre), cam.to_screen(tip));
    f.line(a, b, 1.5, c);
    let back = facing + std::f64::consts::PI;
    for d in [-0.4f64, 0.4] {
        let q = [tip[0] + (back + d).sin() * radius * 0.25, tip[1] + (back + d).cos() * radius * 0.25];
        f.line(b, cam.to_screen(q), 1.5, c);
    }
    f.text(label, a[0] + 6.0, a[1] + 6.0, style::FONT, c);
}

/// Supply point (MIL-STD-2525D / APP-6D sustainment control-measure point, simplified): a pin
/// frame whose apex marks the location, "SP" inside, the callsign to the right and, when zoomed
/// in far enough to matter, the dashed circle a vehicle must be inside to be served. `c` is the
/// affiliation colour (friendly).
pub fn supply_point(f: &mut Frame, cam: &Camera, pos: [f64; 2], radius_m: f64, label: &str, c: Rgba) {
    let p = cam.to_screen(pos);
    let r = (radius_m / cam.mpp) as f32;
    if r > 10.0 {
        let n = 32;
        let pts: Vec<P2> = (0..=n)
            .map(|k| {
                let a = k as f64 / n as f64 * std::f64::consts::TAU;
                cam.to_screen([pos[0] + radius_m * a.sin(), pos[1] + radius_m * a.cos()])
            })
            .collect();
        for w in pts.windows(2).step_by(2) {
            f.line(w[0], w[1], 1.0, c);
        }
    }
    let (w, h, tip) = (24.0, 15.0, 9.0);
    let (x0, x1, y1) = (p[0] - w / 2.0, p[0] + w / 2.0, p[1] - tip);
    let y0 = y1 - h;
    f.rect(x0, y0, w, h, Rgba(10, 18, 24, 200));
    let outline = [[x0, y0], [x1, y0], [x1, y1], [p[0], p[1]], [x0, y1]];
    for k in 0..outline.len() {
        f.line(outline[k], outline[(k + 1) % outline.len()], 1.5, c);
    }
    let tw = f.text_width("SP", style::FONT);
    f.text("SP", p[0] - tw / 2.0, y0 + 3.0, style::FONT, c);
    f.text(label, x1 + 4.0, y0 + 3.0, style::FONT, c);
}

/// Hull footprint at true scale (drawn when zoomed in) with the gun line along the turret.
#[allow(clippy::too_many_arguments)]
pub fn hull(f: &mut Frame, cam: &Camera, pos: [f64; 2], heading: f64, turret: f64, length: f64, width: f64, c: Rgba) {
    let (s, co) = (heading.sin(), heading.cos());
    let fwd = [s, co];
    let right = [co, -s];
    let corner =
        |a: f64, b: f64| cam.to_screen([pos[0] + fwd[0] * a + right[0] * b, pos[1] + fwd[1] * a + right[1] * b]);
    let (hl, hw) = (length / 2.0, width / 2.0);
    let q = [corner(hl, -hw), corner(hl, hw), corner(-hl, hw), corner(-hl, -hw)];
    for k in 0..4 {
        f.line(q[k], q[(k + 1) % 4], 1.5, c);
    }
    let gun = [pos[0] + turret.sin() * length * 1.05, pos[1] + turret.cos() * length * 1.05];
    f.line(cam.to_screen(pos), cam.to_screen(gun), 1.5, c);
}

pub fn selection(f: &mut Frame, p: P2) {
    let (x0, y0, x1, y1) = (p[0] - W / 2.0 - 4.0, p[1] - H / 2.0 - 4.0, p[0] + W / 2.0 + 4.0, p[1] + H / 2.0 + 4.0);
    let l = 6.0;
    let c = style::SELECT;
    for (cx, cy, dx, dy) in [(x0, y0, 1.0, 1.0), (x1, y0, -1.0, 1.0), (x0, y1, 1.0, -1.0), (x1, y1, -1.0, -1.0)] {
        f.line([cx, cy], [cx + dx * l, cy], 1.5, c);
        f.line([cx, cy], [cx, cy + dy * l], 1.5, c);
    }
}

/// Control-measure graphics (MIL-STD-2525D tactical graphics, simplified).
pub fn graphic(f: &mut Frame, cam: &Camera, g: &sim::view::Graphic, highlight: bool) {
    use sim::view::GraphicKind;
    let c = if highlight { style::SELECT } else { Rgba(200, 200, 200, 230) };
    match &g.kind {
        GraphicKind::PhaseLine { points } => {
            let pts: Vec<P2> = points.iter().map(|&p| cam.to_screen(p)).collect();
            for w in pts.windows(2) {
                f.line(w[0], w[1], 2.0, c);
            }
            for p in [pts.first(), pts.last()].into_iter().flatten() {
                f.text(&g.label, p[0] + 4.0, p[1] - 14.0, style::FONT, c);
            }
        }
        GraphicKind::Objective { centre, radius } => {
            let n = 40;
            let pts: Vec<P2> = (0..=n)
                .map(|k| {
                    let a = k as f64 / n as f64 * std::f64::consts::TAU;
                    cam.to_screen([centre[0] + radius * a.sin(), centre[1] + radius * a.cos()])
                })
                .collect();
            for w in pts.windows(2) {
                f.line(w[0], w[1], 2.0, c);
            }
            let p = cam.to_screen(*centre);
            f.text(&g.label, p[0] - 16.0, p[1] - 5.0, style::FONT_L - 8, c);
        }
        GraphicKind::Trp { at } => {
            let p = cam.to_screen(*at);
            f.circle_lines(p, 6.0, c);
            f.line([p[0] - 8.0, p[1]], [p[0] + 8.0, p[1]], 1.0, c);
            f.line([p[0], p[1] - 8.0], [p[0], p[1] + 8.0], 1.0, c);
            f.text(&g.label, p[0] + 9.0, p[1] + 2.0, style::FONT, c);
        }
        GraphicKind::BattlePosition { centre, radius, facing } => {
            battle_position(f, cam, *centre, *radius, *facing, &g.label, true)
        }
    }
}
