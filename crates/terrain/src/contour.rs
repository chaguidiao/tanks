//! Contour extraction: marching squares over the node grid, segment joining into polylines,
//! Douglas–Peucker simplification. Output order is deterministic (by level, then scan order).

use std::collections::{BTreeMap, HashMap};

use crate::Heightfield;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ContourKind {
    /// Thick, labelled (every 50 m by default).
    Index = 0,
    /// Every 10 m by default.
    Intermediate = 1,
    /// Every 5 m by default; drawn only at high zoom.
    Supplementary = 2,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContourLine {
    pub level: f32,
    pub kind: ContourKind,
    pub closed: bool,
    /// Metric coordinates.
    pub points: Vec<[f32; 2]>,
}

#[derive(Debug, Clone, Copy)]
pub struct ContourSpec {
    pub interval: f64,
    pub intermediate_every: f64,
    pub index_every: f64,
    /// Douglas–Peucker tolerance (m).
    pub simplify_m: f64,
}

impl Default for ContourSpec {
    fn default() -> Self {
        Self { interval: 5.0, intermediate_every: 10.0, index_every: 50.0, simplify_m: 0.6 }
    }
}

impl ContourSpec {
    fn kind(&self, level: f64) -> ContourKind {
        let is_multiple = |m: f64| {
            let r = level / m;
            (r - libm::round(r)).abs() < 1e-6
        };
        if is_multiple(self.index_every) {
            ContourKind::Index
        } else if is_multiple(self.intermediate_every) {
            ContourKind::Intermediate
        } else {
            ContourKind::Supplementary
        }
    }
}

// Edge ids: horizontal edge (i,j)-(i+1,j) = 2*(j*w+i); vertical edge (i,j)-(i,j+1) = 2*(j*w+i)+1.
const B: u8 = 0; // bottom
const R: u8 = 1; // right
const T: u8 = 2; // top
const L: u8 = 3; // left

fn case_segments(case: u8, centre_inside: bool) -> &'static [(u8, u8)] {
    match case {
        1 | 14 => &[(L, B)],
        2 | 13 => &[(B, R)],
        3 | 12 => &[(L, R)],
        4 | 11 => &[(R, T)],
        6 | 9 => &[(B, T)],
        7 | 8 => &[(L, T)],
        5 => {
            if centre_inside {
                &[(B, R), (L, T)]
            } else {
                &[(L, B), (R, T)]
            }
        }
        10 => {
            if centre_inside {
                &[(L, B), (R, T)]
            } else {
                &[(B, R), (L, T)]
            }
        }
        _ => &[],
    }
}

pub fn extract(hf: &Heightfield, spec: &ContourSpec) -> Vec<ContourLine> {
    let z = &hf.z;
    let (w, h) = (z.w, z.h);
    if w < 2 || h < 2 {
        return Vec::new();
    }
    let iv = spec.interval;
    let edge = |i: usize, j: usize, side: u8| -> u64 {
        match side {
            B => 2 * (j * w + i) as u64,
            T => 2 * ((j + 1) * w + i) as u64,
            L => 2 * (j * w + i) as u64 + 1,
            _ => 2 * (j * w + i + 1) as u64 + 1,
        }
    };
    let mut by_level: BTreeMap<i64, Vec<(u64, u64)>> = BTreeMap::new();
    for j in 0..h - 1 {
        for i in 0..w - 1 {
            let c = [z.get(i, j), z.get(i + 1, j), z.get(i + 1, j + 1), z.get(i, j + 1)];
            let lo = c.iter().copied().fold(f32::INFINITY, f32::min) as f64;
            let hi = c.iter().copied().fold(f32::NEG_INFINITY, f32::max) as f64;
            let k0 = libm::floor(lo / iv) as i64 + 1;
            let k1 = libm::floor(hi / iv) as i64;
            for k in k0..=k1 {
                let lv = k as f64 * iv;
                if lv <= lo {
                    continue;
                }
                let mut case = 0u8;
                for (b, &v) in c.iter().enumerate() {
                    if v as f64 >= lv {
                        case |= 1 << b;
                    }
                }
                let centre = c.iter().map(|&v| v as f64).sum::<f64>() / 4.0 >= lv;
                for &(a, b) in case_segments(case, centre) {
                    by_level.entry(k).or_default().push((edge(i, j, a), edge(i, j, b)));
                }
            }
        }
    }

    let point = |e: u64, lv: f64| -> [f32; 2] {
        let n = (e / 2) as usize;
        let (i, j) = (n % w, n / w);
        let (i2, j2) = if e.is_multiple_of(2) { (i + 1, j) } else { (i, j + 1) };
        let (za, zb) = (z.get(i, j) as f64, z.get(i2, j2) as f64);
        let t = if zb != za { ((lv - za) / (zb - za)).clamp(0.0, 1.0) } else { 0.5 };
        let x = (i as f64 + 0.5 + (i2 as f64 - i as f64) * t) * hf.cell;
        let y = (j as f64 + 0.5 + (j2 as f64 - j as f64) * t) * hf.cell;
        [x as f32, y as f32]
    };

    let mut out = Vec::new();
    for (k, segs) in by_level {
        let lv = k as f64 * iv;
        let kind = spec.kind(lv);
        // Edge -> up to two segment indices (lookups only; never iterated).
        let mut at: HashMap<u64, [u32; 2]> = HashMap::with_capacity(segs.len() * 2);
        for (s, &(a, b)) in segs.iter().enumerate() {
            for e in [a, b] {
                let slot = at.entry(e).or_insert([u32::MAX; 2]);
                if slot[0] == u32::MAX {
                    slot[0] = s as u32;
                } else {
                    slot[1] = s as u32;
                }
            }
        }
        let other = |e: u64, s: u32| -> Option<u32> {
            let slot = at.get(&e)?;
            let o = if slot[0] == s { slot[1] } else { slot[0] };
            (o != u32::MAX).then_some(o)
        };
        let mut used = vec![false; segs.len()];
        for start in 0..segs.len() {
            if used[start] {
                continue;
            }
            used[start] = true;
            let (a, b) = segs[start];
            // Walk forward from b.
            let mut fwd = vec![a, b];
            let (mut cur_seg, mut cur_edge) = (start as u32, b);
            let mut closed = false;
            while let Some(n) = other(cur_edge, cur_seg) {
                if used[n as usize] {
                    closed = n as usize == start;
                    break;
                }
                used[n as usize] = true;
                let (na, nb) = segs[n as usize];
                let next = if na == cur_edge { nb } else { na };
                fwd.push(next);
                cur_seg = n;
                cur_edge = next;
            }
            if !closed {
                // Walk backward from a.
                let mut back = Vec::new();
                let (mut cur_seg, mut cur_edge) = (start as u32, a);
                while let Some(n) = other(cur_edge, cur_seg) {
                    if used[n as usize] {
                        break;
                    }
                    used[n as usize] = true;
                    let (na, nb) = segs[n as usize];
                    let next = if na == cur_edge { nb } else { na };
                    back.push(next);
                    cur_seg = n;
                    cur_edge = next;
                }
                back.reverse();
                back.extend(fwd);
                fwd = back;
            } else {
                fwd.pop(); // last edge repeats the first
            }
            let pts: Vec<[f32; 2]> = fwd.iter().map(|&e| point(e, lv)).collect();
            let pts = simplify(&pts, spec.simplify_m as f32, closed);
            if pts.len() >= 2 {
                out.push(ContourLine { level: lv as f32, kind, closed, points: pts });
            }
        }
    }
    out
}

/// Douglas–Peucker polyline simplification.
pub fn simplify(pts: &[[f32; 2]], tol: f32, closed: bool) -> Vec<[f32; 2]> {
    if pts.len() < 4 || tol <= 0.0 {
        return pts.to_vec();
    }
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    let last = pts.len() - 1;
    keep[last] = true;
    if closed {
        // Anchor the farthest point from the start so both halves simplify independently.
        let far =
            (1..pts.len()).max_by(|&a, &b| dist2(pts[0], pts[a]).total_cmp(&dist2(pts[0], pts[b]))).unwrap_or(last);
        keep[far] = true;
        dp(pts, 0, far, tol * tol, &mut keep);
        dp(pts, far, last, tol * tol, &mut keep);
    } else {
        dp(pts, 0, last, tol * tol, &mut keep);
    }
    pts.iter().zip(keep).filter(|(_, k)| *k).map(|(p, _)| *p).collect()
}

fn dist2(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)
}

fn seg_dist2(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    if l2 == 0.0 {
        return dist2(p, a);
    }
    let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0);
    dist2(p, [a[0] + t * dx, a[1] + t * dy])
}

fn dp(pts: &[[f32; 2]], s: usize, e: usize, tol2: f32, keep: &mut [bool]) {
    let mut stack = vec![(s, e)];
    while let Some((s, e)) = stack.pop() {
        if e <= s + 1 {
            continue;
        }
        let (mut best, mut bi) = (0.0f32, s);
        for i in s + 1..e {
            let d = seg_dist2(pts[i], pts[s], pts[e]);
            if d > best {
                best = d;
                bi = i;
            }
        }
        if best > tol2 {
            keep[bi] = true;
            stack.push((s, bi));
            stack.push((bi, e));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Grid;

    fn cone(n: usize, cell: f64) -> Heightfield {
        let c = n as f64 * cell / 2.0;
        let mut g = Grid::new(n, n, 0.0f32);
        for j in 0..n {
            for i in 0..n {
                let (x, y) = ((i as f64 + 0.5) * cell - c, (j as f64 + 0.5) * cell - c);
                g.set(i, j, (100.0 - libm::sqrt(x * x + y * y) * 0.5) as f32);
            }
        }
        Heightfield::new(g, cell)
    }

    #[test]
    fn cone_gives_closed_circles_at_correct_radius() {
        let hf = cone(128, 4.0);
        let lines = extract(&hf, &ContourSpec::default());
        let l80: Vec<_> = lines.iter().filter(|l| l.level == 80.0).collect();
        assert_eq!(l80.len(), 1, "one ring at 80 m");
        let ring = l80[0];
        assert!(ring.closed);
        assert_eq!(ring.kind, ContourKind::Intermediate);
        // z = 100 - r/2 = 80  =>  r = 40 m around the centre (256, 256).
        for p in &ring.points {
            let r = ((p[0] - 256.0).powi(2) + (p[1] - 256.0).powi(2)).sqrt();
            assert!((r - 40.0).abs() < 1.5, "r = {r}");
        }
        assert!(lines.iter().any(|l| l.level == 100.0 - 50.0 && l.kind == ContourKind::Index));
    }

    #[test]
    fn plane_gives_open_straight_lines() {
        let mut g = Grid::new(32, 32, 0.0f32);
        for j in 0..32 {
            for i in 0..32 {
                g.set(i, j, i as f32 * 1.0);
            }
        }
        let lines = extract(&Heightfield::new(g, 4.0), &ContourSpec::default());
        assert!(!lines.is_empty());
        for l in &lines {
            assert!(!l.closed);
            assert_eq!(l.points.len(), 2, "straight line simplifies to two points");
        }
    }
}
