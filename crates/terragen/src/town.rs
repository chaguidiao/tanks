//! Stage 6c — one market town (TERRAIN.md §3 stage 6, §5.4; plan `docs/plans/M8H1B-TOWNS.md`).
//!
//! The town is a lattice of street nodes around a centre, rotated to a random axis. Near the
//! centre the lattice is bent and jittered (an old core grown street by street); outside it is
//! regular (planned extensions), after Parish & Müller (2001) and Kelly & McCabe (2006). Each
//! lattice quad gets a zone from its distance and bearing from the centre, after the urban zones
//! of FM 3-06 (2006) *Urban Operations* App. B and FM 90-10-1 (1993) ch. 1: old core (closed
//! perimeter blocks of terraced masonry around courtyards, a market square with a church),
//! residential (houses in rows with gardens), a high-rise estate (concrete slab blocks) and an
//! industrial sector (sheds and yards). Quads of one block merge; the edges between blocks are
//! the streets. The routed road network ends at the town's edge and joins the streets there.

use std::collections::{BTreeMap, BTreeSet};

use sim_data::TerrainData;
use terrain::features::{Building, BuildingClass, BuildingProps, River, Road, RoadClass, RoadProps};
use terrain::{Grid, Heightfield, Vec2, noise, raster};

use crate::recipe::TownParams;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Zone {
    Square,
    Core,
    Residential,
    HighRise,
    Industrial,
}

type Node = (i32, i32);

/// A block: one or more lattice quads of one zone, as a counter-clockwise ring of street nodes.
#[derive(Debug, Clone)]
pub struct Block {
    pub zone: Zone,
    pub ring: Vec<Vec2>,
    /// Half the width of the street along each ring edge (edge k runs ring[k] → ring[k + 1]).
    pub half: Vec<f64>,
}

#[derive(Debug, Clone)]
pub struct TownPlan {
    pub centre: Vec2,
    pub p: TownParams,
    pub blocks: Vec<Block>,
    /// Street centrelines and their widths.
    pub streets: Vec<(Vec<Vec2>, f64)>,
    /// Lattice quads kept (their corners), for the land cover.
    pub quads: Vec<[Vec2; 4]>,
    /// Street nodes on the town's outer edge: where routed roads join.
    pub gates: Vec<Vec2>,
}

fn dist(a: Vec2, b: Vec2) -> f64 {
    libm::sqrt((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2))
}

fn seg_dist(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    let t = if l2 == 0.0 { 0.0 } else { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0) };
    dist(p, [a[0] + dx * t, a[1] + dy * t])
}

fn line_dist(p: Vec2, pts: &[Vec2]) -> f64 {
    pts.windows(2).map(|w| seg_dist(p, w[0], w[1])).fold(f64::INFINITY, f64::min)
}

fn inside(p: Vec2, ring: &[Vec2]) -> bool {
    let mut c = false;
    for k in 0..ring.len() {
        let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
            c = !c;
        }
    }
    c
}

fn signed_area(ring: &[Vec2]) -> f64 {
    (0..ring.len())
        .map(|k| {
            let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        / 2.0
}

/// Pick the town centre: flat, dry, open ground in the middle of the map, best beside (not on)
/// a river, so the town sits at a crossing.
pub fn place(
    hf: &Heightfield,
    landcover: &Grid<u8>,
    rivers: &[River],
    p: &TownParams,
    td: &TerrainData,
    seed: u64,
) -> Vec2 {
    let size = hf.z.w as f64 * hf.cell;
    let cell = hf.cell;
    let lc_at = |x: f64, y: f64| {
        let (i, j) = (((x / cell) as usize).min(landcover.w - 1), ((y / cell) as usize).min(landcover.h - 1));
        td.landcover(landcover.get(i, j))
    };
    let mut best = (f64::NEG_INFINITY, [size / 2.0, size / 2.0]);
    let step = 100.0;
    let n = (size * 0.4 / step) as i64;
    for b in 0..=n {
        for a in 0..=n {
            let c = [size * 0.3 + a as f64 * step, size * 0.3 + b as f64 * step];
            let (mut slope, mut wet, mut k) = (0.0, 0.0, 0.0);
            for v in -3..=3 {
                for u in -3..=3 {
                    let q = [c[0] + u as f64 * p.radius_m / 3.0, c[1] + v as f64 * p.radius_m / 3.0];
                    if dist(q, c) > p.radius_m {
                        continue;
                    }
                    let (gx, gy) = hf.gradient(q[0], q[1]);
                    slope += libm::sqrt(gx * gx + gy * gy);
                    let lc = lc_at(q[0], q[1]);
                    if lc.is_water || lc.name == "marsh" {
                        wet += 1.0;
                    }
                    k += 1.0;
                }
            }
            let river = rivers.iter().map(|r| line_dist(c, &r.points)).fold(f64::INFINITY, f64::min);
            let crossing = if river < p.core_radius_m + 60.0 {
                -2.0
            } else if river < p.radius_m * 1.1 {
                0.6
            } else {
                0.0
            };
            let off = dist(c, [size / 2.0, size / 2.0]) / size;
            let score = -slope / k * 25.0 - wet / k * 3.0 - off + crossing + 0.2 * noise::hash01(a, b, seed);
            if score > best.0 {
                best = (score, c);
            }
        }
    }
    best.1
}

/// Lay out the town around `centre`: lattice, zones, blocks and streets.
pub fn layout(
    centre: Vec2,
    p: &TownParams,
    hf: &Heightfield,
    landcover: &Grid<u8>,
    rivers: &[River],
    td: &TerrainData,
    seed: u64,
) -> TownPlan {
    let b = p.block_m;
    let size = hf.z.w as f64 * hf.cell;
    let theta = noise::hash01(1, 2, seed) * std::f64::consts::FRAC_PI_2;
    let (st, ct) = (libm::sin(theta), libm::cos(theta));
    let world = |l: Vec2| [centre[0] + ct * l[0] - st * l[1], centre[1] + st * l[0] + ct * l[1]];
    let undeformed = |(a, c): Node| [(a as f64 + 0.5) * b, (c as f64 + 0.5) * b];
    // Organic core: bend and jitter fade out over one block beyond the core radius.
    let node_pos = |n: Node| {
        let l = undeformed(n);
        let r = libm::sqrt(l[0] * l[0] + l[1] * l[1]);
        let k = ((p.core_radius_m + b - r) / b).clamp(0.0, 1.0);
        let phi = k * 0.15 * libm::sin(std::f64::consts::TAU * r / (3.0 * b));
        let (sp, cp) = (libm::sin(phi), libm::cos(phi));
        let jx = (noise::hash01(n.0 as i64, n.1 as i64, seed ^ 0x51) - 0.5) * 2.0 * p.core_jitter_m * k;
        let jy = (noise::hash01(n.0 as i64, n.1 as i64, seed ^ 0x52) - 0.5) * 2.0 * p.core_jitter_m * k;
        world([cp * l[0] - sp * l[1] + jx, sp * l[0] + cp * l[1] + jy])
    };
    let span = (p.radius_m / b).ceil() as i32 + 1;
    let mut nodes: BTreeMap<Node, Vec2> = BTreeMap::new();
    for c in -span..=span {
        for a in -span..=span {
            let l = undeformed((a, c));
            // A ragged outline: the edge radius wanders by about a fifth with bearing.
            let bearing = libm::atan2(l[1], l[0]);
            let edge = p.radius_m
                * (1.0 + 0.3 * noise::gradient(libm::cos(bearing) * 1.5, libm::sin(bearing) * 1.5, seed ^ 0x77));
            if libm::sqrt(l[0] * l[0] + l[1] * l[1]) <= edge {
                let w = node_pos((a, c));
                if w[0] > 30.0 && w[1] > 30.0 && w[0] < size - 30.0 && w[1] < size - 30.0 {
                    nodes.insert((a, c), w);
                }
            }
        }
    }
    let cell = hf.cell;
    let lc_at = |q: Vec2| {
        let (i, j) = (((q[0] / cell) as usize).min(landcover.w - 1), ((q[1] / cell) as usize).min(landcover.h - 1));
        td.landcover(landcover.get(i, j))
    };
    let river_near = |q: Vec2| {
        rivers.iter().any(|r| {
            let half = r.props.width_m.iter().copied().fold(0.0f32, f32::max) as f64 / 2.0;
            line_dist(q, &r.points) < half + 30.0
        })
    };
    let h_sector = noise::hash01(3, 4, seed) * std::f64::consts::TAU;
    let i_sector = h_sector + std::f64::consts::PI;
    let in_sector = |alpha: f64, mid: f64, width_deg: f64| {
        let d = (alpha - mid).rem_euclid(std::f64::consts::TAU);
        let d = d.min(std::f64::consts::TAU - d);
        d <= width_deg.to_radians() / 2.0
    };
    let corners = |(a, c): Node| [(a, c), (a + 1, c), (a + 1, c + 1), (a, c + 1)];
    let mut quads: BTreeMap<Node, Zone> = BTreeMap::new();
    for &(a, c) in nodes.keys() {
        let cs = corners((a, c));
        let Some(pts) = cs.iter().map(|n| nodes.get(n).copied()).collect::<Option<Vec<_>>>() else { continue };
        let mid = [pts.iter().map(|q| q[0]).sum::<f64>() / 4.0, pts.iter().map(|q| q[1]).sum::<f64>() / 4.0];
        let (gx, gy) = hf.gradient(mid[0], mid[1]);
        if libm::sqrt(gx * gx + gy * gy) > 0.15
            || lc_at(mid).is_water
            || pts.iter().chain([&mid]).any(|&q| river_near(q))
        {
            continue;
        }
        let l = [(a + 1) as f64 * b, (c + 1) as f64 * b];
        let r = libm::sqrt(l[0] * l[0] + l[1] * l[1]);
        let alpha = libm::atan2(l[1], l[0]);
        let zone = if (a, c) == (-1, -1) {
            Zone::Square
        } else if r < p.core_radius_m {
            Zone::Core
        } else if in_sector(alpha, h_sector, p.highrise_sector_deg) {
            Zone::HighRise
        } else if in_sector(alpha, i_sector, p.industrial_sector_deg) {
            Zone::Industrial
        } else {
            Zone::Residential
        };
        quads.insert((a, c), zone);
    }
    let street_w = |z: Zone| match z {
        Zone::Square | Zone::Core => p.core_street_m,
        _ => p.street_m,
    };

    // Blocks: residential quads pair up along the axis, high-rise and industrial in 2 x 2.
    let mut groups: BTreeMap<(Zone, i32, i32), Vec<Node>> = BTreeMap::new();
    for (&(a, c), &z) in &quads {
        let key = match z {
            Zone::Square | Zone::Core => (z, a, c),
            Zone::Residential => (z, a.div_euclid(2), c),
            Zone::HighRise | Zone::Industrial => (z, a.div_euclid(2), c.div_euclid(2)),
        };
        groups.entry(key).or_default().push((a, c));
    }
    // Split each group into 4-connected components.
    let mut comps: Vec<(Zone, Vec<Node>)> = Vec::new();
    for ((z, _, _), qs) in groups {
        let mut left: BTreeSet<Node> = qs.into_iter().collect();
        while let Some(&start) = left.iter().next() {
            left.remove(&start);
            let (mut comp, mut stack) = (vec![start], vec![start]);
            while let Some((a, c)) = stack.pop() {
                for nb in [(a + 1, c), (a - 1, c), (a, c + 1), (a, c - 1)] {
                    if left.remove(&nb) {
                        comp.push(nb);
                        stack.push(nb);
                    }
                }
            }
            comp.sort();
            comps.push((z, comp));
        }
    }
    let mut owner: BTreeMap<Node, usize> = BTreeMap::new();
    for (k, (_, qs)) in comps.iter().enumerate() {
        for &q in qs {
            owner.insert(q, k);
        }
    }
    // The quads either side of a lattice edge: (left, right) of the directed edge u → v.
    let sides = |u: Node, v: Node| -> (Node, Node) {
        if v.1 == u.1 {
            // Horizontal edge at row u.1: quad above has key (min a, u.1), below (min a, u.1 - 1).
            let a = u.0.min(v.0);
            if v.0 > u.0 { ((a, u.1), (a, u.1 - 1)) } else { ((a, u.1 - 1), (a, u.1)) }
        } else {
            let c = u.1.min(v.1);
            if v.1 > u.1 { ((u.0 - 1, c), (u.0, c)) } else { ((u.0, c), (u.0 - 1, c)) }
        }
    };
    let mut street_edges: BTreeMap<(Node, Node), f64> = BTreeMap::new();
    let mut blocks = Vec::new();
    for (k, (z, qs)) in comps.iter().enumerate() {
        // Boundary edges of the union, directed counter-clockwise (block on the left).
        let mut next: BTreeMap<Node, Node> = BTreeMap::new();
        for &q in qs {
            let cs = corners(q);
            for e in 0..4 {
                let (u, v) = (cs[e], cs[(e + 1) % 4]);
                let (_, right) = sides(u, v);
                if owner.get(&right) == Some(&k) {
                    continue;
                }
                next.insert(u, v);
                let w = quads.get(&right).map_or(street_w(*z), |&o| street_w(o).max(street_w(*z)));
                let key = if u < v { (u, v) } else { (v, u) };
                let e = street_edges.entry(key).or_insert(w);
                *e = e.max(w);
            }
        }
        let Some(&start) = next.keys().next() else { continue };
        let mut ring_nodes = vec![start];
        let mut cur = next[&start];
        while cur != start && ring_nodes.len() <= next.len() {
            ring_nodes.push(cur);
            cur = next[&cur];
        }
        let ring: Vec<Vec2> = ring_nodes.iter().map(|n| nodes[n]).collect();
        let half = (0..ring_nodes.len())
            .map(|e| {
                let (u, v) = (ring_nodes[e], ring_nodes[(e + 1) % ring_nodes.len()]);
                let key = if u < v { (u, v) } else { (v, u) };
                street_edges[&key] / 2.0
            })
            .collect();
        blocks.push(Block { zone: *z, ring, half });
    }

    // Streets: chain the edges along each lattice line into polylines of one width.
    let mut lines: BTreeMap<(bool, i32), Vec<(i32, f64)>> = BTreeMap::new();
    for (&(u, v), &w) in &street_edges {
        if u.1 == v.1 {
            lines.entry((true, u.1)).or_default().push((u.0.min(v.0), w));
        } else {
            lines.entry((false, u.0)).or_default().push((u.1.min(v.1), w));
        }
    }
    let mut streets = Vec::new();
    for ((horiz, fixed), mut es) in lines {
        es.sort_by_key(|e| e.0);
        let node = |t: i32| if horiz { (t, fixed) } else { (fixed, t) };
        let mut run: Vec<Vec2> = Vec::new();
        let mut run_w = 0.0;
        let mut last = i32::MIN;
        for (t, w) in es {
            if run.is_empty() || t != last + 1 || w != run_w {
                if run.len() >= 2 {
                    streets.push((std::mem::take(&mut run), run_w));
                }
                run = vec![nodes[&node(t)]];
                run_w = w;
            }
            run.push(nodes[&node(t + 1)]);
            last = t;
        }
        if run.len() >= 2 {
            streets.push((run, run_w));
        }
    }
    // Gates: street nodes with a lattice neighbour that is not a street node (the outer edge).
    let street_nodes: BTreeSet<Node> = street_edges.keys().flat_map(|&(u, v)| [u, v]).collect();
    let gates = street_nodes
        .iter()
        .filter(|&&(a, c)| {
            quads.keys().filter(|&&(qa, qc)| (qa == a || qa == a - 1) && (qc == c || qc == c - 1)).count() < 4
        })
        .map(|n| nodes[n])
        .collect();
    let quads = quads.keys().map(|&q| corners(q).map(|n| nodes[&n])).collect();
    TownPlan { centre, p: p.clone(), blocks, streets, quads, gates }
}

/// Paint the town's land cover over every kept quad.
pub fn stamp(landcover: &mut Grid<u8>, plan: &TownPlan, cell: f64, td: &TerrainData) {
    let id = td.landcover_id("town").expect("landcover 'town' missing");
    let (w, h) = (landcover.w, landcover.h);
    for q in &plan.quads {
        raster::fill_polygon(q, cell, w, h, |i, j| landcover.set(i, j, id));
    }
}

/// End the routed roads at the town's edge: a road that runs into the town is cut where it enters
/// and joined to the nearest gate; the streets carry the traffic inside. A road that crosses the
/// town (rare: the router avoids town ground) becomes two roads, the second numbered `next_id`.
pub fn connect(roads: Vec<Road>, plan: &TownPlan, mut next_id: u32) -> Vec<Road> {
    let r_in = plan.p.radius_m * 0.98;
    let nearest_gate = |q: Vec2| plan.gates.iter().copied().min_by(|a, b| dist(*a, q).total_cmp(&dist(*b, q)));
    let mut out_roads = Vec::with_capacity(roads.len());
    for road in roads {
        let mut parts: Vec<Vec<Vec2>> = vec![Vec::new()];
        let mut was_in = false;
        for &q in &road.points {
            let is_in = dist(q, plan.centre) < r_in;
            let cur = parts.last_mut().unwrap();
            match (was_in, is_in) {
                (false, false) => cur.push(q),
                (false, true) => cur.extend(nearest_gate(q)),
                (true, false) => {
                    if !cur.is_empty() {
                        parts.push(Vec::new());
                    }
                    let cur = parts.last_mut().unwrap();
                    cur.extend(nearest_gate(q));
                    cur.push(q);
                }
                (true, true) => {}
            }
            was_in = is_in;
        }
        let mut first = true;
        for mut pts in parts {
            pts.dedup();
            if pts.len() < 2 {
                continue;
            }
            let id = if first {
                road.id
            } else {
                next_id += 1;
                next_id - 1
            };
            first = false;
            out_roads.push(Road { id, points: pts, props: road.props.clone() });
        }
    }
    out_roads
}

/// The town's streets as road features, numbered from `first_id`.
pub fn streets(plan: &TownPlan, first_id: u32) -> Vec<Road> {
    plan.streets
        .iter()
        .enumerate()
        .map(|(k, (pts, w))| Road {
            id: first_id + k as u32,
            points: pts.clone(),
            props: RoadProps { class: RoadClass::Street, width_m: *w as f32 },
        })
        .collect()
}

/// Inset a counter-clockwise ring: each edge moves inward by its own offset, corners mitred.
/// Nearly straight corners are dropped first (offset = the larger of the two edges').
fn inset(ring: &[Vec2], off: &[f64]) -> Option<Vec<Vec2>> {
    let (mut r, mut o) = (ring.to_vec(), off.to_vec());
    loop {
        let n = r.len();
        if n < 3 {
            return None;
        }
        let straight = (0..n).find(|&k| {
            let (a, b, c) = (r[(k + n - 1) % n], r[k], r[(k + 1) % n]);
            let (u, v) = ([b[0] - a[0], b[1] - a[1]], [c[0] - b[0], c[1] - b[1]]);
            let cr = u[0] * v[1] - u[1] * v[0];
            let dt = u[0] * v[0] + u[1] * v[1];
            libm::atan2(cr, dt).abs() < 12f64.to_radians()
        });
        let Some(k) = straight else { break };
        let prev = (k + n - 1) % n;
        o[prev] = o[prev].max(o[k]);
        r.remove(k);
        o.remove(k);
    }
    let n = r.len();
    let line = |k: usize| {
        let (a, b) = (r[k], r[(k + 1) % n]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l = libm::sqrt(dx * dx + dy * dy);
        let nrm = [-dy / l, dx / l];
        ([a[0] + nrm[0] * o[k], a[1] + nrm[1] * o[k]], [dx / l, dy / l])
    };
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        let (p1, d1) = line((k + n - 1) % n);
        let (p2, d2) = line(k);
        let den = d1[0] * d2[1] - d1[1] * d2[0];
        if libm::fabs(den) < 1e-6 {
            return None;
        }
        let t = ((p2[0] - p1[0]) * d2[1] - (p2[1] - p1[1]) * d2[0]) / den;
        out.push([p1[0] + d1[0] * t, p1[1] + d1[1] * t]);
    }
    (signed_area(&out) > 50.0).then_some(out)
}

/// Rectangle `len` along `u`, `dep` along the inward normal, front edge centred at `front`.
fn rect(front: Vec2, u: Vec2, len: f64, dep: f64) -> Vec<Vec2> {
    let n = [-u[1], u[0]];
    let c = |a: f64, d: f64| [front[0] + u[0] * a + n[0] * d, front[1] + u[1] * a + n[1] * d];
    // Counter-clockwise: front right, back right, back left, front left.
    vec![c(len / 2.0, 0.0), c(len / 2.0, dep), c(-len / 2.0, dep), c(-len / 2.0, 0.0)]
}

/// Building footprints for every block, numbered from `first_id`. `roads` are the routed roads
/// (kept clear); buildings also keep off rivers and the map edge.
pub fn buildings(
    plan: &TownPlan,
    roads: &[Road],
    rivers: &[River],
    size_m: f64,
    seed: u64,
    first_id: u32,
) -> Vec<Building> {
    let reach = plan.p.radius_m + 80.0;
    let road_segs: Vec<(Vec2, Vec2, f64)> = roads
        .iter()
        .flat_map(|r| r.points.windows(2).map(move |w| (w[0], w[1], r.props.width_m as f64 / 2.0 + 2.0)))
        .filter(|(a, b, _)| dist(*a, plan.centre) < reach || dist(*b, plan.centre) < reach)
        .collect();
    let river_segs: Vec<(Vec2, Vec2, f64)> = rivers
        .iter()
        .flat_map(|r| {
            let half = r.props.width_m.iter().copied().fold(0.0f32, f32::max) as f64 / 2.0 + 15.0;
            r.points.windows(2).map(move |w| (w[0], w[1], half))
        })
        .filter(|(a, b, _)| dist(*a, plan.centre) < reach + 200.0 || dist(*b, plan.centre) < reach + 200.0)
        .collect();
    let clear = |ring: &[Vec2]| {
        let mid = [ring.iter().map(|q| q[0]).sum::<f64>() / 4.0, ring.iter().map(|q| q[1]).sum::<f64>() / 4.0];
        ring.iter().chain([&mid]).all(|&q| {
            q[0] > 20.0
                && q[1] > 20.0
                && q[0] < size_m - 20.0
                && q[1] < size_m - 20.0
                && road_segs.iter().chain(&river_segs).all(|&(a, b, r)| seg_dist(q, a, b) > r)
        })
    };
    let mut out: Vec<Building> = Vec::new();
    let mut n = 0u64;
    let push = |ring: Vec<Vec2>, height: f64, class: BuildingClass, out: &mut Vec<Building>| {
        if clear(&ring) {
            let id = first_id + out.len() as u32;
            out.push(Building { id, ring, props: BuildingProps { height_m: height as f32, class } });
        }
    };
    for blk in &plan.blocks {
        let setback = match blk.zone {
            Zone::Square | Zone::Core => 0.5,
            Zone::Residential => 4.0,
            Zone::HighRise => 10.0,
            Zone::Industrial => 6.0,
        };
        let off: Vec<f64> = blk.half.iter().map(|h| h + setback).collect();
        let Some(lot) = inset(&blk.ring, &off) else { continue };
        let m = lot.len();
        let edges: Vec<(Vec2, Vec2, f64)> = (0..m)
            .map(|k| {
                let (a, b) = (lot[k], lot[(k + 1) % m]);
                let l = dist(a, b);
                (a, [(b[0] - a[0]) / l, (b[1] - a[1]) / l], l)
            })
            .collect();
        let mut h = |k: u64| {
            n += 1;
            noise::hash01(n as i64, k as i64, seed)
        };
        match blk.zone {
            Zone::Core => {
                // Closed perimeter rows: terraced town houses, 2-4 storeys, frontage 12-16 m.
                // Each row starts a little inside the previous row's corner house, so the
                // corners close and the courtyard is shut to vehicles.
                for &(a, u, l) in &edges {
                    let depth = 11.0 + 2.0 * h(1);
                    let s = depth * 0.3;
                    if l < s + 6.0 {
                        continue;
                    }
                    let count = ((l - s) / 14.0).round().max(1.0);
                    let f = (l - s) / count;
                    for i in 0..count as usize {
                        let along = s + (i as f64 + 0.5) * f;
                        let front = [a[0] + u[0] * along, a[1] + u[1] * along];
                        let floors = 2.0 + (h(2) * 3.0).floor().min(2.0);
                        push(rect(front, u, f, depth), floors * 3.5, BuildingClass::Masonry, &mut out);
                    }
                }
            }
            Zone::Square => {
                // The market square stays open; a church stands on it.
                let c = [
                    lot.iter().map(|q| q[0]).sum::<f64>() / m as f64,
                    lot.iter().map(|q| q[1]).sum::<f64>() / m as f64,
                ];
                let (_, u, _) = edges[0];
                let n_in = [-u[1], u[0]];
                let front = [c[0] - n_in[0] * 7.0, c[1] - n_in[1] * 7.0];
                push(rect(front, u, 30.0, 14.0), 16.0, BuildingClass::Masonry, &mut out);
            }
            Zone::Residential => {
                // Houses with gardens along the long sides, gaps between them.
                let longest = edges.iter().map(|e| e.2).fold(0.0, f64::max);
                for &(a, u, l) in &edges {
                    if l < longest * 0.6 {
                        continue;
                    }
                    let depth = 9.0 + h(1);
                    let mut along = depth + 4.0;
                    while along + 10.0 < l - depth - 4.0 {
                        let f = 10.0 + 3.0 * h(2);
                        let front = [a[0] + u[0] * (along + f / 2.0), a[1] + u[1] * (along + f / 2.0)];
                        let class = if h(3) < 0.15 { BuildingClass::Wood } else { BuildingClass::Masonry };
                        let height = 6.0 + 2.5 * h(4);
                        push(rect(front, u, f, depth), height, class, &mut out);
                        along += f + 8.0 + 6.0 * h(5);
                    }
                }
            }
            Zone::HighRise | Zone::Industrial => {
                // Rows parallel to the longest lot edge; a footprint is kept only if it lies
                // inside the lot.
                let (a, u, l) = edges.iter().copied().max_by(|x, y| x.2.total_cmp(&y.2)).unwrap();
                let nrm = [-u[1], u[0]];
                let (len, dep, gap_along, gap_across) =
                    if blk.zone == Zone::HighRise { (54.0, 12.0, 26.0, 34.0) } else { (40.0, 24.0, 16.0, 16.0) };
                // Start just inside the lot edge so the first row tests as inside.
                let mut across = 1.0;
                for _ in 0..8 {
                    let mut along = 1.0;
                    while along + len <= l + 1.0 {
                        let (ln, dp) =
                            if blk.zone == Zone::HighRise { (len, dep) } else { (len - 14.0 * h(1), dep - 6.0 * h(2)) };
                        let front = [
                            a[0] + u[0] * (along + ln / 2.0) + nrm[0] * across,
                            a[1] + u[1] * (along + ln / 2.0) + nrm[1] * across,
                        ];
                        let ring = rect(front, u, ln, dp);
                        if ring.iter().all(|&q| inside(q, &lot)) {
                            let (height, class) = if blk.zone == Zone::HighRise {
                                (if h(3) < 0.3 { 12.8 } else { 16.0 }, BuildingClass::Concrete)
                            } else {
                                (8.0 + 2.0 * h(4), BuildingClass::Industrial)
                            };
                            push(ring, height, class, &mut out);
                        }
                        along += len + gap_along;
                    }
                    across += dep + gap_across;
                }
            }
        }
    }
    out
}
