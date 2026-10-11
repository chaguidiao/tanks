//! Route following for a single vehicle.
//!
//! Doctrine: ATP 3-20.15 (2025), Ch. 3 "Tactical movement" — a vehicle moving along an assigned
//! route at the ordered speed, slowing for turns and halting at the end point. Formation keeping
//! and movement techniques (traveling / overwatch / bounding) build on this in M4.
//!
//! Steering is pure pursuit (Coulter 1992, CMU-RI-TR-92-01): aim at a point one look-ahead
//! distance down the path; the commanded curvature is 2·sin(α)/L.

pub type Vec2 = [f64; 2];

#[derive(Debug, Clone, Copy)]
pub struct DriveParams {
    /// Ordered or governed top speed (m/s).
    pub max_speed: f64,
    /// Lateral acceleration the driver accepts in turns (m/s²).
    pub max_lateral_accel: f64,
    /// Deceleration used to plan the stop at the end point (m/s²).
    pub braking_decel: f64,
    /// Above this heading error the driver slows to a crawl and pivots (rad).
    pub pivot_threshold: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DriveCmd {
    /// Desired forward speed (m/s, ≥ 0).
    pub speed: f64,
    /// Desired compass heading (rad, clockwise from north).
    pub heading: f64,
    /// Index of the route segment the vehicle is on (monotonic).
    pub segment: usize,
    pub arrived: bool,
}

/// Distance at which the end point counts as reached (m).
pub const ARRIVAL_RADIUS: f64 = 4.0;

fn sub(a: Vec2, b: Vec2) -> Vec2 {
    [a[0] - b[0], a[1] - b[1]]
}
fn len(a: Vec2) -> f64 {
    libm::sqrt(a[0] * a[0] + a[1] * a[1])
}

/// Compass bearing from `a` to `b` (rad, 0 = north, clockwise).
pub fn bearing(a: Vec2, b: Vec2) -> f64 {
    libm::atan2(b[0] - a[0], b[1] - a[1])
}

/// Wrap an angle to (-π, π].
pub fn wrap(a: f64) -> f64 {
    let t = std::f64::consts::TAU;
    let mut a = a % t;
    if a <= -std::f64::consts::PI {
        a += t;
    } else if a > std::f64::consts::PI {
        a -= t;
    }
    a
}

/// One decision step. `segment` is the index of the segment the vehicle was last on.
pub fn drive(pos: Vec2, heading: f64, speed: f64, route: &[Vec2], segment: usize, p: &DriveParams) -> DriveCmd {
    let Some(&goal) = route.last() else {
        return DriveCmd { speed: 0.0, heading, segment, arrived: true };
    };
    let to_goal = len(sub(goal, pos));
    if to_goal < ARRIVAL_RADIUS {
        return DriveCmd { speed: 0.0, heading, segment, arrived: true };
    }
    // Advance the segment index while the vehicle has passed the segment's end.
    let mut seg = segment.min(route.len().saturating_sub(2));
    while seg + 2 < route.len() {
        let (a, b) = (route[seg], route[seg + 1]);
        let ab = sub(b, a);
        let l2 = ab[0] * ab[0] + ab[1] * ab[1];
        let t = if l2 > 0.0 { ((pos[0] - a[0]) * ab[0] + (pos[1] - a[1]) * ab[1]) / l2 } else { 1.0 };
        if t >= 1.0 || len(sub(b, pos)) < ARRIVAL_RADIUS { seg += 1 } else { break }
    }
    // Look-ahead point: walk the path from the projection of `pos` on the current segment.
    let look = (4.0 + 1.2 * speed.abs()).clamp(5.0, 30.0);
    let target = lookahead(pos, route, seg, look);
    let desired = bearing(pos, target);
    let err = wrap(desired - heading);

    let mut v = p.max_speed;
    // Pure-pursuit curvature k = 2 sin(α) / L  ⇒  corner speed v ≤ sqrt(a_lat / k).
    let k = 2.0 * libm::sin(err.abs()) / look;
    if k > 1e-6 {
        v = v.min(libm::sqrt(p.max_lateral_accel / k));
    }
    if err.abs() > p.pivot_threshold {
        v = v.min(1.0); // crawl and pivot towards the route
    }
    // Anticipate the next corners: arrive at each at a speed suited to its turn angle, so the
    // look-ahead (which shrinks with speed) is short where the route bends.
    let mut along = len(sub(route[(seg + 1).min(route.len() - 1)], pos));
    for c in seg + 1..route.len().saturating_sub(1) {
        let turn = wrap(bearing(route[c], route[c + 1]) - bearing(route[c - 1], route[c])).abs();
        if turn > 0.15 {
            // Corner speed: 2 m/s for a right angle, scaling with the turn.
            let v_corner = (2.0 * (std::f64::consts::FRAC_PI_2 / turn)).clamp(1.5, p.max_speed);
            v = v.min(libm::sqrt(v_corner * v_corner + 2.0 * p.braking_decel * 0.6 * along));
        }
        along += len(sub(route[c + 1], route[c]));
        if along > 150.0 {
            break;
        }
    }
    // Plan the stop: v ≤ sqrt(2 a d) over the remaining path length.
    let remaining = remaining_length(pos, route, seg);
    v = v.min(libm::sqrt(2.0 * p.braking_decel * 0.6 * (remaining - ARRIVAL_RADIUS * 0.5).max(0.0)) + 0.5);
    DriveCmd { speed: v.max(0.0), heading: desired, segment: seg, arrived: false }
}

fn lookahead(pos: Vec2, route: &[Vec2], seg: usize, dist: f64) -> Vec2 {
    let (a, b) = (route[seg], route[(seg + 1).min(route.len() - 1)]);
    let ab = sub(b, a);
    let l2 = ab[0] * ab[0] + ab[1] * ab[1];
    let t = if l2 > 0.0 { (((pos[0] - a[0]) * ab[0] + (pos[1] - a[1]) * ab[1]) / l2).clamp(0.0, 1.0) } else { 1.0 };
    let mut cur = [a[0] + ab[0] * t, a[1] + ab[1] * t];
    let mut left = dist;
    for &p in &route[seg + 1..] {
        let d = len(sub(p, cur));
        if d >= left {
            let f = left / d;
            return [cur[0] + (p[0] - cur[0]) * f, cur[1] + (p[1] - cur[1]) * f];
        }
        left -= d;
        cur = p;
    }
    *route.last().unwrap()
}

fn remaining_length(pos: Vec2, route: &[Vec2], seg: usize) -> f64 {
    let mut d = len(sub(route[(seg + 1).min(route.len() - 1)], pos));
    for k in seg + 1..route.len().saturating_sub(1) {
        d += len(sub(route[k + 1], route[k]));
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: DriveParams =
        DriveParams { max_speed: 15.0, max_lateral_accel: 3.0, braking_decel: 4.0, pivot_threshold: 1.2 };

    #[test]
    fn bearing_convention_is_compass() {
        assert!((bearing([0.0, 0.0], [0.0, 10.0]) - 0.0).abs() < 1e-12); // north
        assert!((bearing([0.0, 0.0], [10.0, 0.0]) - std::f64::consts::FRAC_PI_2).abs() < 1e-12); // east
    }

    #[test]
    fn straight_route_gives_full_speed_on_course() {
        let c = drive([0.0, 0.0], 0.0, 10.0, &[[0.0, 0.0], [0.0, 1000.0]], 0, &P);
        assert!(!c.arrived);
        assert!((c.speed - 15.0).abs() < 1e-9);
        assert!(c.heading.abs() < 1e-9);
    }

    #[test]
    fn pivots_when_facing_away_and_stops_at_goal() {
        let c = drive([0.0, 0.0], std::f64::consts::PI, 0.0, &[[0.0, 0.0], [0.0, 1000.0]], 0, &P);
        assert!(c.speed <= 1.0);
        let c = drive([0.0, 998.0], 0.0, 1.0, &[[0.0, 0.0], [0.0, 1000.0]], 0, &P);
        assert!(c.arrived && c.speed == 0.0);
    }

    #[test]
    fn slows_down_before_the_end() {
        let near = drive([0.0, 980.0], 0.0, 10.0, &[[0.0, 0.0], [0.0, 1000.0]], 0, &P);
        let far = drive([0.0, 500.0], 0.0, 10.0, &[[0.0, 0.0], [0.0, 1000.0]], 0, &P);
        assert!(near.speed < far.speed && near.speed < 12.0);
    }
}
