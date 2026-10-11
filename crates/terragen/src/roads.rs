//! Stage 6 — villages and the road network (TERRAIN.md §3).
//!
//! Village sites are scored for flat, moderately dry, open ground and picked greedily with a
//! minimum spacing. Roads connect villages along a minimum spanning tree (main roads) plus
//! relative-neighbourhood-graph links (secondary roads), and run to a few map-edge exits.
//! Each road is a least-cost route over a slope-penalised grid that discourages river
//! crossings; roads built earlier are cheap to reuse, so the network merges naturally.

use sim_data::TerrainData;
use terrain::features::{River, Road, RoadClass, RoadProps};
use terrain::path::Pathfinder;
use terrain::{Grid, Heightfield, Vec2, noise, raster};

use crate::hydro::Flow;
use crate::recipe::RoadParams;

#[derive(Debug, Clone, Copy)]
pub struct Village {
    pub pos: Vec2,
    pub radius: f64,
}

fn dist(a: Vec2, b: Vec2) -> f64 {
    libm::sqrt((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2))
}

pub fn place_villages(
    hf: &Heightfield,
    flow: &Flow,
    landcover: &Grid<u8>,
    p: &RoadParams,
    td: &TerrainData,
    seed: u64,
) -> Vec<Village> {
    let (w, h) = (hf.z.w, hf.z.h);
    let cell = hf.cell;
    let size = w as f64 * cell;
    let step = ((p.village_spacing_m / 12.0) / cell).max(4.0) as usize;
    let margin = (p.village_radius_m * 2.0).max(300.0);
    let _ = flow;
    let mut cands = Vec::new();
    for j in (step / 2..h).step_by(step) {
        for i in (step / 2..w).step_by(step) {
            let pos = [(i as f64 + 0.5) * cell, (j as f64 + 0.5) * cell];
            if pos[0] < margin || pos[1] < margin || pos[0] > size - margin || pos[1] > size - margin {
                continue;
            }
            let lc = td.landcover(landcover.get(i, j));
            if lc.is_forest || lc.name == "marsh" {
                continue;
            }
            let s = hf.node_slope(i, j);
            let score = 1.0 - s * 12.0 + 0.4 * noise::hash01(i as i64, j as i64, seed);
            cands.push((score, i, j, pos));
        }
    }
    cands.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.2.cmp(&b.2)).then(a.1.cmp(&b.1)));
    let target = ((size / p.village_spacing_m).powi(2)).round().max(1.0) as usize;
    let mut out: Vec<Village> = Vec::new();
    for (_, i, j, pos) in cands {
        if out.len() >= target {
            break;
        }
        if out.iter().all(|v| dist(v.pos, pos) >= p.village_spacing_m * 0.7) {
            let r = p.village_radius_m * (0.7 + 0.6 * noise::hash01(i as i64, j as i64, seed ^ 1));
            out.push(Village { pos, radius: r });
        }
    }
    out
}

pub fn stamp_villages(landcover: &mut Grid<u8>, villages: &[Village], cell: f64, td: &TerrainData, seed: u64) {
    let vid = td.landcover_id("village").expect("landcover 'village' missing");
    for (n, v) in villages.iter().enumerate() {
        let r = v.radius;
        let (i0, i1) = (
            ((v.pos[0] - r * 1.4) / cell).max(0.0) as usize,
            (((v.pos[0] + r * 1.4) / cell) as usize).min(landcover.w),
        );
        let (j0, j1) = (
            ((v.pos[1] - r * 1.4) / cell).max(0.0) as usize,
            (((v.pos[1] + r * 1.4) / cell) as usize).min(landcover.h),
        );
        for j in j0..j1 {
            for i in i0..i1 {
                let (x, y) = ((i as f64 + 0.5) * cell, (j as f64 + 0.5) * cell);
                let wobble = 1.0 + 0.35 * noise::gradient(x / 120.0, y / 120.0, seed ^ n as u64);
                if dist([x, y], v.pos) <= r * wobble {
                    landcover.set(i, j, vid);
                }
            }
        }
    }
}

const COARSE: usize = 4;

pub fn build_network(
    hf: &Heightfield,
    landcover: &Grid<u8>,
    rivers: &[River],
    villages: &[Village],
    p: &RoadParams,
    td: &TerrainData,
    first_id: u32,
) -> Vec<Road> {
    let cell = hf.cell * COARSE as f64;
    let (w, h) = (hf.z.w / COARSE, hf.z.h / COARSE);
    let size = hf.z.w as f64 * hf.cell;
    // River mask on the coarse grid.
    let mut river = Grid::new(w, h, false);
    for r in rivers {
        let half = r.props.width_m.iter().copied().fold(0.0f32, f32::max) as f64 / 2.0 + cell * 0.5;
        raster::near_polyline(&r.points, half, cell, w, h, |i, j, _, _| river.set(i, j, true));
    }
    // "Speed" grid for the router: higher = cheaper to build along.
    let mut speed = Grid::new(w, h, 1u8);
    for j in 0..h {
        for i in 0..w {
            let (x, y) = ((i as f64 + 0.5) * cell, (j as f64 + 0.5) * cell);
            let (gx, gy) = hf.gradient(x, y);
            let s = libm::sqrt(gx * gx + gy * gy);
            let (fi, fj) = (i * COARSE, j * COARSE);
            let lc = td.landcover(landcover.get(fi.min(landcover.w - 1), fj.min(landcover.h - 1)));
            let mut v = 100.0 / (1.0 + (s / 0.04).powi(2));
            if lc.is_forest {
                v *= 0.5;
            }
            if lc.name == "marsh" {
                v *= 0.25;
            }
            if lc.name == "village" {
                v = v.max(60.0);
            }
            if river.get(i, j) {
                v *= 0.12; // bridges are expensive: cross rarely, and where it is cheapest
            }
            speed.set(i, j, v.clamp(1.0, 120.0) as u8);
        }
    }

    // Graph: MST over villages (main), then RNG links (secondary), then edge exits (main).
    let n = villages.len();
    let mut links: Vec<(usize, usize, RoadClass)> = Vec::new();
    if n > 1 {
        let mut in_tree = vec![false; n];
        let mut best = vec![(f64::INFINITY, usize::MAX); n];
        in_tree[0] = true;
        for k in 1..n {
            best[k] = (dist(villages[0].pos, villages[k].pos), 0);
        }
        for _ in 1..n {
            let (k, _) = (0..n)
                .filter(|&k| !in_tree[k])
                .map(|k| (k, best[k].0))
                .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
                .unwrap();
            in_tree[k] = true;
            links.push((best[k].1, k, RoadClass::Main));
            for m in 0..n {
                if !in_tree[m] {
                    let d = dist(villages[k].pos, villages[m].pos);
                    if d < best[m].0 {
                        best[m] = (d, k);
                    }
                }
            }
        }
        let mut extra = Vec::new();
        for a in 0..n {
            for b in a + 1..n {
                if links.iter().any(|&(x, y, _)| (x, y) == (a, b) || (x, y) == (b, a)) {
                    continue;
                }
                let dab = dist(villages[a].pos, villages[b].pos);
                if dab > p.village_spacing_m * 2.2 {
                    continue;
                }
                let blocked = (0..n).any(|c| {
                    c != a
                        && c != b
                        && dist(villages[a].pos, villages[c].pos).max(dist(villages[b].pos, villages[c].pos)) < dab
                });
                if !blocked {
                    extra.push((dab, a, b));
                }
            }
        }
        extra.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)).then(x.2.cmp(&y.2)));
        links.extend(extra.into_iter().map(|(_, a, b)| (a, b, RoadClass::Secondary)));
    }
    let mut ends: Vec<Vec2> = villages.iter().map(|v| v.pos).collect();
    for e in 0..p.edge_exits {
        let t = 0.2 + 0.6 * noise::hash01(e as i64, 7, first_id as u64 ^ 0xed6e);
        let inset = cell * 0.5;
        let pos = match e % 4 {
            0 => [t * size, inset],
            1 => [size - inset, t * size],
            2 => [t * size, size - inset],
            _ => [inset, t * size],
        };
        let idx = ends.len();
        ends.push(pos);
        let target = (0..n).min_by(|&a, &b| dist(pos, villages[a].pos).total_cmp(&dist(pos, villages[b].pos)));
        match target {
            Some(v) => links.push((idx, v, RoadClass::Main)),
            None if idx > 0 => links.push((idx, idx - 1, RoadClass::Main)),
            None => {}
        }
    }

    let mut pf = Pathfinder::new();
    let mut roads = Vec::new();
    for (a, b, class) in links {
        let Some(route) = pf.find(&speed, cell, ends[a], ends[b]) else { continue };
        // Make this road cheap for later roads so the network shares corridors.
        for wpt in route.windows(2) {
            raster::near_polyline(wpt, cell * 0.5, cell, w, h, |i, j, _, _| speed.set(i, j, 140));
        }
        let pts = chaikin(&chaikin(&route));
        let width = match class {
            RoadClass::Main => 7.0,
            RoadClass::Secondary => 5.0,
            RoadClass::Track => 3.5,
        };
        roads.push(Road { id: first_id + roads.len() as u32, points: pts, props: RoadProps { class, width_m: width } });
    }
    roads
}

fn chaikin(p: &[Vec2]) -> Vec<Vec2> {
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
