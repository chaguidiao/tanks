//! NATO symbology subset (MIL-STD-2525D / APP-6D) drawn from primitives: frame shape encodes
//! affiliation (rectangle friendly, diamond hostile), the ellipse is the armour icon.

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

#[cfg_attr(not(feature = "debug-truth"), allow(dead_code))]
pub fn hostile_armour(f: &mut Frame, p: P2, c: Rgba) {
    let r = H * 0.9;
    let pts = [[p[0], p[1] - r], [p[0] + r, p[1]], [p[0], p[1] + r], [p[0] - r, p[1]]];
    for k in 0..4 {
        f.line(pts[k], pts[(k + 1) % 4], 2.0, c);
    }
    f.ellipse_lines(p, r * 0.45, r * 0.28, c);
}

/// Direction-of-movement staff from the symbol centre.
pub fn heading_staff(f: &mut Frame, p: P2, heading: f64, len: f32, c: Rgba) {
    let (s, co) = (heading.sin() as f32, heading.cos() as f32);
    let a = [p[0] + s * (H / 2.0), p[1] - co * (H / 2.0)];
    let b = [p[0] + s * (H / 2.0 + len), p[1] - co * (H / 2.0 + len)];
    f.line(a, b, 1.5, c);
}

/// Hull footprint at true scale (drawn when zoomed in) with the gun line.
pub fn hull(f: &mut Frame, cam: &Camera, pos: [f64; 2], heading: f64, length: f64, width: f64, c: Rgba) {
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
    f.line(cam.to_screen(pos), corner(hl + length * 0.55, 0.0), 1.5, c);
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
