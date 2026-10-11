//! Stage 2 — flow routing and erosion.
//!
//! - Priority-Flood with flow directions (Barnes, Lehman & Mulla 2014): every cell gets a
//!   receiver that drains to the map edge, including across depressions and flats.
//! - Stream-power erosion with the implicit O(n) scheme of Braun & Willett (2013), n = 1.
//! - Thermal erosion to a talus slope (Musgrave, Kolb & Mace 1989).

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use terrain::Heightfield;

use crate::recipe::ErosionParams;

pub struct Flow {
    /// Downstream neighbour of each cell (itself for outlets on the map edge).
    pub receiver: Vec<u32>,
    /// Cells in flood order: outlets first, every cell after its receiver.
    pub order: Vec<u32>,
    /// Drainage area (m²) including the cell itself.
    pub area: Vec<f32>,
    /// Depression-filled elevation.
    pub filled: Vec<f32>,
}

/// Total order on f32 as u32 (for heap keys).
fn key(v: f32) -> u32 {
    let b = v.to_bits();
    if b & 0x8000_0000 != 0 { !b } else { b | 0x8000_0000 }
}

const NB: [(i64, i64); 8] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)];

impl Flow {
    pub fn compute(hf: &Heightfield) -> Self {
        let (w, h) = (hf.z.w, hf.z.h);
        let n = w * h;
        let z = &hf.z.data;
        let mut receiver = vec![u32::MAX; n];
        let mut filled = z.clone();
        let mut order = Vec::with_capacity(n);
        let mut heap = BinaryHeap::with_capacity(4 * (w + h));
        let mut seq: u64 = 0;
        for j in 0..h {
            for i in 0..w {
                if i == 0 || j == 0 || i == w - 1 || j == h - 1 {
                    let k = j * w + i;
                    receiver[k] = k as u32;
                    heap.push((Reverse(key(z[k])), Reverse(seq), k as u32));
                    seq += 1;
                }
            }
        }
        while let Some((_, _, c)) = heap.pop() {
            let c = c as usize;
            order.push(c as u32);
            let (ci, cj) = ((c % w) as i64, (c / w) as i64);
            for (dx, dy) in NB {
                let (ni, nj) = (ci + dx, cj + dy);
                if ni < 0 || nj < 0 || ni >= w as i64 || nj >= h as i64 {
                    continue;
                }
                let k = nj as usize * w + ni as usize;
                if receiver[k] != u32::MAX {
                    continue;
                }
                receiver[k] = c as u32;
                filled[k] = filled[k].max(filled[c]);
                heap.push((Reverse(key(filled[k])), Reverse(seq), k as u32));
                seq += 1;
            }
        }
        let cell_area = (hf.cell * hf.cell) as f32;
        let mut area = vec![cell_area; n];
        for &c in order.iter().rev() {
            let r = receiver[c as usize];
            if r != c {
                area[r as usize] += area[c as usize];
            }
        }
        Self { receiver, order, area, filled }
    }
}

pub fn erode(hf: &mut Heightfield, p: &ErosionParams) {
    let w = hf.z.w;
    let cell = hf.cell;
    for _ in 0..p.iterations {
        let flow = Flow::compute(hf);
        let z = &mut hf.z.data;
        for &c in &flow.order {
            let c = c as usize;
            let r = flow.receiver[c] as usize;
            if r == c {
                continue;
            }
            let diag = (c % w != r % w) && (c / w != r / w);
            let dist = if diag { cell * std::f64::consts::SQRT_2 } else { cell };
            let f = p.k * libm::pow(flow.area[c] as f64, p.m) / dist;
            let hr = z[r] as f64;
            let hc = z[c] as f64;
            // Implicit update; only erode (never raise a cell above its old height).
            let new = (hc + f * hr) / (1.0 + f);
            if new < hc {
                z[c] = new as f32;
            }
        }
    }
    thermal(hf, p.talus, p.thermal_iterations);
}

fn thermal(hf: &mut Heightfield, talus: f64, iterations: u32) {
    let (w, h) = (hf.z.w, hf.z.h);
    let lim = (talus * hf.cell) as f32;
    let mut delta = vec![0.0f32; w * h];
    for _ in 0..iterations {
        delta.iter_mut().for_each(|d| *d = 0.0);
        let z = &hf.z.data;
        for j in 0..h {
            for i in 0..w {
                let k = j * w + i;
                for (dx, dy) in [(1i64, 0i64), (0, 1)] {
                    let (ni, nj) = (i as i64 + dx, j as i64 + dy);
                    if ni >= w as i64 || nj >= h as i64 {
                        continue;
                    }
                    let m = nj as usize * w + ni as usize;
                    let d = z[k] - z[m];
                    if d.abs() > lim {
                        let mv = (d.abs() - lim) * 0.25 * d.signum();
                        delta[k] -= mv;
                        delta[m] += mv;
                    }
                }
            }
        }
        for (v, d) in hf.z.data.iter_mut().zip(&delta) {
            *v += d;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use terrain::Grid;

    #[test]
    fn every_cell_drains_to_the_edge() {
        // Bowl with a pit in the middle: priority-flood must still route the pit to the edge.
        let n = 40;
        let mut g = Grid::new(n, n, 0.0f32);
        for j in 0..n {
            for i in 0..n {
                let (x, y) = (i as f32 - 20.0, j as f32 - 20.0);
                g.set(i, j, 50.0 - (x * x + y * y).sqrt() * 0.1 + if (x * x + y * y) < 9.0 { -10.0 } else { 0.0 });
            }
        }
        let hf = Heightfield::new(g, 4.0);
        let f = Flow::compute(&hf);
        for start in 0..n * n {
            let mut c = start;
            let mut steps = 0;
            while f.receiver[c] as usize != c {
                c = f.receiver[c] as usize;
                steps += 1;
                assert!(steps < n * n, "cycle");
            }
            let (i, j) = (c % n, c / n);
            assert!(i == 0 || j == 0 || i == n - 1 || j == n - 1);
        }
        // Total area at outlets equals map area.
        let total: f32 = (0..n * n).filter(|&c| f.receiver[c] as usize == c).map(|c| f.area[c]).sum();
        assert!((total - (n * n) as f32 * 16.0).abs() < 1.0);
    }
}
