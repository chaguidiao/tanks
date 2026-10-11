//! Rasterisation helpers shared by the baker and the generator.

use crate::Vec2;

/// Visit every cell (i, j) whose centre lies inside the polygon (even–odd rule).
pub fn fill_polygon(ring: &[Vec2], cell: f64, w: usize, h: usize, mut f: impl FnMut(usize, usize)) {
    if ring.len() < 3 {
        return;
    }
    let (mut ymin, mut ymax) = (f64::INFINITY, f64::NEG_INFINITY);
    for p in ring {
        ymin = ymin.min(p[1]);
        ymax = ymax.max(p[1]);
    }
    let j0 = (libm::floor(ymin / cell - 0.5).max(0.0)) as usize;
    let j1 = (libm::ceil(ymax / cell - 0.5).max(0.0) as usize).min(h.saturating_sub(1));
    let mut xs = Vec::new();
    for j in j0..=j1 {
        let y = (j as f64 + 0.5) * cell;
        xs.clear();
        for k in 0..ring.len() {
            let a = ring[k];
            let b = ring[(k + 1) % ring.len()];
            if (a[1] <= y) != (b[1] <= y) {
                xs.push(a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0]));
            }
        }
        xs.sort_by(f64::total_cmp);
        for pair in xs.chunks(2) {
            if pair.len() < 2 {
                break;
            }
            let i0 = libm::ceil(pair[0] / cell - 0.5).max(0.0) as usize;
            let i1 = libm::floor(pair[1] / cell - 0.5);
            if i1 < 0.0 {
                continue;
            }
            for i in i0..=(i1 as usize).min(w - 1) {
                f(i, j);
            }
        }
    }
}

/// For each cell within `radius` of the polyline, report (i, j, distance, s) where `s` is the
/// along-line distance (m) of the closest point. Cells may be reported more than once (once per
/// nearby segment); callers keep the minimum distance.
pub fn near_polyline(
    pts: &[Vec2],
    radius: f64,
    cell: f64,
    w: usize,
    h: usize,
    mut f: impl FnMut(usize, usize, f64, f64),
) {
    let mut s0 = 0.0;
    for k in 0..pts.len().saturating_sub(1) {
        let (a, b) = (pts[k], pts[k + 1]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = libm::sqrt(dx * dx + dy * dy);
        let i0 = libm::floor((a[0].min(b[0]) - radius) / cell).max(0.0) as usize;
        let i1 = (libm::ceil((a[0].max(b[0]) + radius) / cell).max(0.0) as usize).min(w);
        let j0 = libm::floor((a[1].min(b[1]) - radius) / cell).max(0.0) as usize;
        let j1 = (libm::ceil((a[1].max(b[1]) + radius) / cell).max(0.0) as usize).min(h);
        for j in j0..j1 {
            for i in i0..i1 {
                let (px, py) = ((i as f64 + 0.5) * cell, (j as f64 + 0.5) * cell);
                let t =
                    if len > 0.0 { (((px - a[0]) * dx + (py - a[1]) * dy) / (len * len)).clamp(0.0, 1.0) } else { 0.0 };
                let (cx, cy) = (a[0] + t * dx, a[1] + t * dy);
                let d = libm::sqrt((px - cx).powi(2) + (py - cy).powi(2));
                if d <= radius {
                    f(i, j, d, s0 + t * len);
                }
            }
        }
        s0 += len;
    }
}

pub fn polyline_length(pts: &[Vec2]) -> f64 {
    pts.windows(2).map(|w| libm::sqrt((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2))).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_fill_counts_cells() {
        let ring = [[8.0, 8.0], [48.0, 8.0], [48.0, 48.0], [8.0, 48.0]];
        let mut n = 0;
        fill_polygon(&ring, 4.0, 100, 100, |_, _| n += 1);
        assert_eq!(n, 10 * 10); // centres 10, 14, ..., 46 in each axis
    }
}
