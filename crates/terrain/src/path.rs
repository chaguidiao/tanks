//! Route planning on a speed raster: 8-connected A* minimising travel time, then string-pulling.
//!
//! Deterministic: integer costs (ms), heap ties broken by insertion order. The full-resolution
//! grid is searched directly; HPA* (DESIGN.md §9.3) replaces this when maps or query counts grow.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::mobility::SPEED_QUANTUM;
use crate::{Grid, Vec2};

/// Reusable search buffers (avoids re-allocating ~9 bytes per cell per query).
#[derive(Debug, Default, Clone)]
pub struct Pathfinder {
    g: Vec<u32>,
    parent: Vec<u32>,
    stamp: Vec<u32>,
    closed: Vec<u32>,
    generation: u32,
}

const SQRT2: f64 = std::f64::consts::SQRT_2;
/// Speed (m/s) assumed when leaving a no-go start cell (e.g. a tank already bogged in a ditch).
const ESCAPE_SPEED: f64 = 0.5;

impl Pathfinder {
    pub fn new() -> Self {
        Self::default()
    }

    fn prepare(&mut self, n: usize) {
        if self.g.len() != n {
            self.g = vec![0; n];
            self.parent = vec![0; n];
            self.stamp = vec![0; n];
            self.closed = vec![0; n];
            self.generation = 0;
        }
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.stamp.iter_mut().for_each(|s| *s = 0);
            self.closed.iter_mut().for_each(|s| *s = 0);
            self.generation = 1;
        }
    }

    /// Fastest route from `start` through each checkpoint in `via` to `goal`, one leg at a time.
    /// Returns the joined waypoints and the arc length (m) at which each checkpoint is reached,
    /// or `None` if any leg is unreachable.
    pub fn find_via(
        &mut self,
        speed: &Grid<u8>,
        cell: f64,
        start: Vec2,
        via: &[Vec2],
        goal: Vec2,
    ) -> Option<(Vec<Vec2>, Vec<f64>)> {
        let mut route: Vec<Vec2> = Vec::new();
        let mut marks = Vec::with_capacity(via.len());
        let mut s = 0.0;
        for &to in via.iter().chain(std::iter::once(&goal)) {
            let from = route.last().copied().unwrap_or(start);
            let leg = self.find(speed, cell, from, to)?;
            // The first leg is kept whole, so an empty `via` gives exactly `find`'s route.
            let skip = usize::from(!route.is_empty());
            for p in leg.into_iter().skip(skip) {
                let last = route.last().copied().unwrap_or(p);
                s += ((p[0] - last[0]).powi(2) + (p[1] - last[1]).powi(2)).sqrt();
                route.push(p);
            }
            marks.push(s);
        }
        marks.pop();
        Some((route, marks))
    }

    /// Fastest route from `start` to `goal` (metric). Returns smoothed waypoints including both
    /// ends, or `None` if the goal is unreachable.
    pub fn find(&mut self, speed: &Grid<u8>, cell: f64, start: Vec2, goal: Vec2) -> Option<Vec<Vec2>> {
        let (w, h) = (speed.w, speed.h);
        let to_cell = |p: Vec2| -> (usize, usize) {
            (
                libm::floor(p[0] / cell).clamp(0.0, (w - 1) as f64) as usize,
                libm::floor(p[1] / cell).clamp(0.0, (h - 1) as f64) as usize,
            )
        };
        let s = to_cell(start);
        let mut gl = to_cell(goal);
        let mut goal_pt = goal;
        if speed.get(gl.0, gl.1) == 0 {
            gl = nearest_passable(speed, gl, 64)?;
            goal_pt = [(gl.0 as f64 + 0.5) * cell, (gl.1 as f64 + 0.5) * cell];
        }
        let n = w * h;
        self.prepare(n);
        let cur_gen = self.generation;
        let vmax = speed.data.iter().copied().max().unwrap_or(1).max(1) as f64 * SPEED_QUANTUM;
        // Heuristic: octile distance at top speed, in ms (admissible: floor).
        let heur = |i: usize, j: usize| -> u32 {
            let dx = (i as i64 - gl.0 as i64).unsigned_abs() as f64;
            let dy = (j as i64 - gl.1 as i64).unsigned_abs() as f64;
            let d = (dx.max(dy) - dx.min(dy) + SQRT2 * dx.min(dy)) * cell;
            libm::floor(d / vmax * 1000.0) as u32
        };
        let inv_v = |i: usize, j: usize| -> f64 {
            let q = speed.get(i, j);
            if q == 0 {
                if (i, j) == s { 1.0 / ESCAPE_SPEED } else { f64::INFINITY }
            } else {
                1.0 / (q as f64 * SPEED_QUANTUM)
            }
        };
        let si = s.1 * w + s.0;
        let gi = gl.1 * w + gl.0;
        let mut heap = BinaryHeap::new();
        let mut seq: u64 = 0;
        self.g[si] = 0;
        self.stamp[si] = cur_gen;
        self.parent[si] = si as u32;
        heap.push((Reverse(heur(s.0, s.1)), Reverse(seq), si as u32));
        let mut found = false;
        while let Some((_, _, cur)) = heap.pop() {
            let cur = cur as usize;
            if self.closed[cur] == cur_gen {
                continue;
            }
            self.closed[cur] = cur_gen;
            if cur == gi {
                found = true;
                break;
            }
            let (ci, cj) = (cur % w, cur / w);
            let gc = self.g[cur];
            let iv_c = inv_v(ci, cj);
            for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
                let (ni, nj) = (ci as i64 + dx, cj as i64 + dy);
                if !speed.in_bounds(ni, nj) {
                    continue;
                }
                let (ni, nj) = (ni as usize, nj as usize);
                let iv_n = inv_v(ni, nj);
                if !iv_n.is_finite() {
                    continue;
                }
                let diag = dx != 0 && dy != 0;
                if diag && (speed.get(ni, cj) == 0 || speed.get(ci, nj) == 0) {
                    continue; // no corner cutting past no-go cells
                }
                let d = if diag { cell * SQRT2 } else { cell };
                let cost = libm::round(d * 0.5 * (iv_c + iv_n) * 1000.0) as u32;
                let ng = gc.saturating_add(cost.max(1));
                let ni_ = nj * w + ni;
                if self.stamp[ni_] != cur_gen || ng < self.g[ni_] {
                    self.stamp[ni_] = cur_gen;
                    self.g[ni_] = ng;
                    self.parent[ni_] = cur as u32;
                    seq += 1;
                    heap.push((Reverse(ng.saturating_add(heur(ni, nj))), Reverse(seq), ni_ as u32));
                }
            }
        }
        if !found {
            return None;
        }
        let mut cells = vec![gi];
        let mut c = gi;
        while c != si {
            c = self.parent[c] as usize;
            cells.push(c);
        }
        cells.reverse();
        let mut pts: Vec<Vec2> =
            cells.iter().map(|&c| [((c % w) as f64 + 0.5) * cell, ((c / w) as f64 + 0.5) * cell]).collect();
        pts[0] = start;
        *pts.last_mut().unwrap() = goal_pt;
        Some(smooth(speed, cell, &pts))
    }
}

fn nearest_passable(speed: &Grid<u8>, c: (usize, usize), max_r: i64) -> Option<(usize, usize)> {
    for r in 1..=max_r {
        let mut best: Option<(i64, (usize, usize))> = None;
        for dy in -r..=r {
            for dx in -r..=r {
                if dx.abs() != r && dy.abs() != r {
                    continue;
                }
                let (x, y) = (c.0 as i64 + dx, c.1 as i64 + dy);
                if speed.in_bounds(x, y) && speed.get(x as usize, y as usize) > 0 {
                    let d = dx * dx + dy * dy;
                    if best.is_none_or(|b| d < b.0) {
                        best = Some((d, (x as usize, y as usize)));
                    }
                }
            }
        }
        if let Some((_, p)) = best {
            return Some(p);
        }
    }
    None
}

/// Travel time (s) along a straight segment, sampling the raster every half cell. `None` if it
/// crosses a no-go cell.
pub fn segment_time(speed: &Grid<u8>, cell: f64, a: Vec2, b: Vec2) -> Option<f64> {
    let len = libm::sqrt((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2));
    let n = libm::ceil(len / (cell * 0.5)).max(1.0) as usize;
    let step = len / n as f64;
    let mut t = 0.0;
    for k in 0..n {
        let f = (k as f64 + 0.5) / n as f64;
        let (x, y) = (a[0] + f * (b[0] - a[0]), a[1] + f * (b[1] - a[1]));
        let (i, j) = (libm::floor(x / cell) as i64, libm::floor(y / cell) as i64);
        if !speed.in_bounds(i, j) {
            return None;
        }
        let q = speed.get(i as usize, j as usize);
        if q == 0 {
            if k == 0 {
                t += step / ESCAPE_SPEED;
                continue;
            }
            return None;
        }
        t += step / (q as f64 * SPEED_QUANTUM);
    }
    Some(t)
}

/// Lines offset ±0.75 cell either side of the segment must also be passable, so shortcuts keep
/// clear of no-go edges (vehicles have width and do not track the line perfectly).
fn has_clearance(speed: &Grid<u8>, cell: f64, a: Vec2, b: Vec2) -> bool {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = libm::sqrt(dx * dx + dy * dy);
    if len < 1e-9 {
        return true;
    }
    let (nx, ny) = (-dy / len * cell * 0.75, dx / len * cell * 0.75);
    let n = libm::ceil(len / (cell * 0.5)).max(1.0) as usize;
    for side in [-1.0, 1.0] {
        // Skip the first and last samples: the ends are on the grid path already.
        for k in 1..n {
            let f = k as f64 / n as f64;
            let (x, y) = (a[0] + f * dx + side * nx, a[1] + f * dy + side * ny);
            let (i, j) = (libm::floor(x / cell) as i64, libm::floor(y / cell) as i64);
            if speed.in_bounds(i, j) && speed.get(i as usize, j as usize) == 0 {
                return false;
            }
        }
    }
    true
}

/// Greedy string-pulling: skip intermediate points while the straight segment is passable and
/// no slower than the grid path it replaces.
fn smooth(speed: &Grid<u8>, cell: f64, pts: &[Vec2]) -> Vec<Vec2> {
    if pts.len() <= 2 {
        return pts.to_vec();
    }
    let mut cum = vec![0.0f64; pts.len()];
    for k in 1..pts.len() {
        cum[k] = cum[k - 1] + segment_time(speed, cell, pts[k - 1], pts[k]).unwrap_or(f64::INFINITY);
    }
    let mut out = vec![pts[0]];
    let mut i = 0;
    while i < pts.len() - 1 {
        let mut best = i + 1;
        let mut j = i + 2;
        while j < pts.len() && j <= i + 400 {
            match segment_time(speed, cell, pts[i], pts[j]) {
                Some(t) if t <= (cum[j] - cum[i]) * 1.01 + 0.05 && has_clearance(speed, cell, pts[i], pts[j]) => {
                    best = j
                }
                _ => break,
            }
            j += 1;
        }
        out.push(pts[best]);
        i = best;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn speed_grid(w: usize, h: usize, v: u8) -> Grid<u8> {
        Grid::new(w, h, v)
    }

    #[test]
    fn open_ground_gives_straight_line() {
        let g = speed_grid(64, 64, 100);
        let mut pf = Pathfinder::new();
        let p = pf.find(&g, 4.0, [10.0, 10.0], [200.0, 150.0]).unwrap();
        assert_eq!(p.len(), 2, "{p:?}");
    }

    #[test]
    fn routes_around_a_wall_through_the_gap() {
        let mut g = speed_grid(64, 64, 100);
        for j in 0..60 {
            g.set(32, j, 0); // wall with a gap at the top (j >= 60)
        }
        let mut pf = Pathfinder::new();
        let p = pf.find(&g, 4.0, [40.0, 40.0], [200.0, 40.0]).unwrap();
        assert!(p.iter().any(|q| q[1] >= 60.0 * 4.0), "must pass through the gap: {p:?}");
        for w in p.windows(2) {
            assert!(segment_time(&g, 4.0, w[0], w[1]).is_some(), "segment crosses the wall");
        }
    }

    #[test]
    fn prefers_a_fast_road_detour() {
        // Slow ground (1 m/s) everywhere except a fast road (10 m/s) along row 50.
        let mut g = speed_grid(100, 60, 10);
        for i in 0..100 {
            g.set(i, 50, 100);
        }
        let mut pf = Pathfinder::new();
        let p = pf.find(&g, 4.0, [10.0, 170.0], [390.0, 170.0]).unwrap();
        // Straight across slow ground: 380 s. Via the road (~120 m detour each way at 1 m/s
        // plus 380 m at 10 m/s) is faster, so the route must use row 50 (y ≈ 202 m).
        assert!(p.iter().any(|q| (q[1] - 202.0).abs() < 4.0), "{p:?}");
    }

    #[test]
    fn route_passes_through_checkpoints_in_order() {
        let g = speed_grid(64, 64, 100);
        let mut pf = Pathfinder::new();
        let (start, cp, goal) = ([10.0, 10.0], [200.0, 20.0], [20.0, 200.0]);
        let (p, marks) = pf.find_via(&g, 4.0, start, &[cp], goal).unwrap();
        assert_eq!(marks.len(), 1);
        let k = p.iter().position(|q| (q[0] - cp[0]).hypot(q[1] - cp[1]) < 4.0).expect("visits checkpoint");
        let s: f64 = p[..=k].windows(2).map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])).sum();
        assert!((s - marks[0]).abs() < 1e-6, "{s} vs {}", marks[0]);
        // No checkpoints: identical to `find`.
        let direct = pf.find(&g, 4.0, start, goal).unwrap();
        assert_eq!(pf.find_via(&g, 4.0, start, &[], goal).unwrap().0, direct);
    }

    #[test]
    fn unreachable_goal_returns_none() {
        let mut g = speed_grid(32, 32, 100);
        for j in 0..32 {
            g.set(16, j, 0);
        }
        let mut pf = Pathfinder::new();
        assert!(pf.find(&g, 4.0, [10.0, 10.0], [100.0, 10.0]).is_none());
    }
}
