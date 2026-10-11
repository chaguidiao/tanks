//! Stage 3 — rivers from flow accumulation, with hydraulic geometry (Leopold & Maddock 1953):
//! width = a·A^b, depth = c·A^d (A = drainage area in km², a proxy for discharge).
//! Rivers are emitted as vector features, carved later by the baker.

use terrain::Heightfield;
use terrain::contour::simplify;
use terrain::features::{River, RiverProps};

use crate::hydro::Flow;
use crate::recipe::RiverParams;

pub fn extract(hf: &Heightfield, flow: &Flow, p: &RiverParams) -> Vec<River> {
    let (w, h) = (hf.z.w, hf.z.h);
    let n = w * h;
    let min_a = p.min_area_m2 as f32;
    let channel = |k: usize| flow.area[k] >= min_a;
    // A head is a channel cell with no channel cell draining into it.
    let mut has_upstream = vec![false; n];
    for k in 0..n {
        let r = flow.receiver[k] as usize;
        if r != k && channel(k) {
            has_upstream[r] = true;
        }
    }
    // Process heads from the largest catchment down so main stems are traced first.
    let mut heads: Vec<usize> = (0..n).filter(|&k| channel(k) && !has_upstream[k]).collect();
    heads.sort_by(|&a, &b| {
        let oa = outlet_area(flow, a);
        let ob = outlet_area(flow, b);
        ob.total_cmp(&oa).then(a.cmp(&b))
    });
    let mut visited = vec![false; n];
    let mut rivers = Vec::new();
    let cell = hf.cell;
    let centre = |k: usize| [((k % w) as f64 + 0.5) * cell, ((k / w) as f64 + 0.5) * cell];
    for head in heads {
        let mut cells = vec![head];
        let mut c = head;
        visited[c] = true;
        loop {
            let r = flow.receiver[c] as usize;
            if r == c {
                break;
            }
            cells.push(r);
            if visited[r] {
                break; // joined an existing river
            }
            visited[r] = true;
            c = r;
        }
        let pts: Vec<[f32; 2]> = cells
            .iter()
            .map(|&k| {
                let p = centre(k);
                [p[0] as f32, p[1] as f32]
            })
            .collect();
        let length = cells.len() as f64 * cell;
        if length < p.min_length_m {
            continue;
        }
        let simp = simplify(&pts, (cell * 0.8) as f32, false);
        let pts = chaikin(&simp.iter().map(|q| [q[0] as f64, q[1] as f64]).collect::<Vec<_>>());
        let (mut width, mut depth) = (Vec::new(), Vec::new());
        for q in &pts {
            let (i, j) = hf.cell_of(q[0], q[1]);
            // Use the largest drainage area in the 3x3 neighbourhood (smoothed points may sit off-channel).
            let mut a: f32 = 0.0;
            for dj in -1i64..=1 {
                for di in -1i64..=1 {
                    let (x, y) = (i as i64 + di, j as i64 + dj);
                    if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
                        a = a.max(flow.area[y as usize * w + x as usize]);
                    }
                }
            }
            let a_km2 = (a.max(min_a) as f64) / 1e6;
            width.push((p.width_coef * libm::pow(a_km2, p.width_exp)) as f32);
            depth.push((p.depth_coef * libm::pow(a_km2, p.depth_exp)) as f32);
        }
        rivers.push(River {
            id: rivers.len() as u32 + 1,
            points: pts,
            props: RiverProps { width_m: width, depth_m: depth },
        });
    }
    rivers
}

fn outlet_area(flow: &Flow, mut c: usize) -> f32 {
    let mut steps = 0;
    while flow.receiver[c] as usize != c && steps < 1_000_000 {
        c = flow.receiver[c] as usize;
        steps += 1;
    }
    flow.area[c]
}

/// One round of Chaikin corner cutting (keeps the end points).
fn chaikin(p: &[[f64; 2]]) -> Vec<[f64; 2]> {
    if p.len() < 3 {
        return p.to_vec();
    }
    let mut out = vec![p[0]];
    for w in p.windows(2) {
        let (a, b) = (w[0], w[1]);
        out.push([0.75 * a[0] + 0.25 * b[0], 0.75 * a[1] + 0.25 * b[1]]);
        out.push([0.25 * a[0] + 0.75 * b[0], 0.25 * a[1] + 0.75 * b[1]]);
    }
    out.push(*p.last().unwrap());
    out
}
