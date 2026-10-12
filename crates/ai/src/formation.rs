//! Formation geometry (ATP 3-20.15 (2025) ch. 3, formations; FM 17-15 (1996) ch. 2).
//! Slots are offsets from the platoon leader in a frame aligned with the direction of travel
//! (Pottinger 1999, "Coordinated Unit Movement": virtual leader plus slot offsets).

use crate::picture::Vec2;

/// World position of a slot given leader position, heading (compass rad) and an offset
/// (right, forward) in metres.
pub fn slot_position(leader: Vec2, heading: f64, offset_m: (f64, f64)) -> Vec2 {
    let (s, c) = (libm::sin(heading), libm::cos(heading));
    let fwd = [s, c];
    let right = [c, -s];
    [leader[0] + right[0] * offset_m.0 + fwd[0] * offset_m.1, leader[1] + right[1] * offset_m.0 + fwd[1] * offset_m.1]
}

/// Point and heading at arc length `s` along a polyline.
pub fn along(route: &[Vec2], s: f64) -> (Vec2, f64) {
    if route.len() < 2 {
        return (route.first().copied().unwrap_or([0.0, 0.0]), 0.0);
    }
    let mut left = s.max(0.0);
    for w in route.windows(2) {
        let d = libm::sqrt((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2));
        if left <= d || d == 0.0 {
            let t = if d > 0.0 { left / d } else { 0.0 };
            return (
                [w[0][0] + (w[1][0] - w[0][0]) * t, w[0][1] + (w[1][1] - w[0][1]) * t],
                crate::drive::bearing(w[0], w[1]),
            );
        }
        left -= d;
    }
    let n = route.len();
    (route[n - 1], crate::drive::bearing(route[n - 2], route[n - 1]))
}

/// The part of a polyline beyond arc length `s` (starting at the point at `s`).
pub fn remaining(route: &[Vec2], s: f64) -> Vec<Vec2> {
    let mut out = vec![along(route, s).0];
    let mut acc = 0.0;
    for w in route.windows(2) {
        acc += libm::sqrt((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2));
        if acc > s {
            out.push(w[1]);
        }
    }
    out
}

pub fn length(route: &[Vec2]) -> f64 {
    route.windows(2).map(|w| libm::sqrt((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2))).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remaining_starts_at_progress() {
        let r = [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0]];
        assert_eq!(remaining(&r, 50.0), vec![[50.0, 0.0], [100.0, 0.0], [100.0, 100.0]]);
        assert_eq!(remaining(&r, 150.0), vec![[100.0, 50.0], [100.0, 100.0]]);
        assert_eq!(remaining(&r, 500.0), vec![[100.0, 100.0]]);
    }

    #[test]
    fn slots_rotate_with_heading() {
        // Heading east: "right" is south, "forward" is east.
        let p = slot_position([0.0, 0.0], std::f64::consts::FRAC_PI_2, (100.0, -50.0));
        assert!((p[0] + 50.0).abs() < 1e-9 && (p[1] + 100.0).abs() < 1e-9, "{p:?}");
    }

    #[test]
    fn along_walks_the_route() {
        let r = [[0.0, 0.0], [0.0, 100.0], [100.0, 100.0]];
        let (p, h) = along(&r, 150.0);
        assert!((p[0] - 50.0).abs() < 1e-9 && (p[1] - 100.0).abs() < 1e-9);
        assert!((h - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        assert!((length(&r) - 200.0).abs() < 1e-9);
    }
}
