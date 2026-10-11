//! Battle-position search: find hull-down (or turret-down) firing positions in an area
//! (DESIGN.md §9.2; ATP 3-20.15 (2025) ch. 5 "occupy a battle position"; tactical position
//! evaluation after Straatman, van der Sterren & Beij, "Killzone's AI" (GDC 2005)).
//!
//! Candidates on a jittered grid are scored against observers placed along the threat
//! direction: a candidate is hull-down when no observer sees the hull roof but most see the
//! gunner's sight. Coverage, exposure, gun depression and distance from the centre add to the
//! score. Positions are then picked greedily with lateral dispersion, and vehicles assigned to
//! minimise total travel.

use terrain::ground::Ground;
use terrain::los::sight;

use crate::picture::Vec2;

#[derive(Debug, Clone, Copy)]
pub struct BpQuery {
    pub centre: Vec2,
    pub radius: f64,
    /// Threat direction (compass rad).
    pub facing: f64,
    pub hull_height: f64,
    pub sight_height: f64,
    /// Maximum gun depression (positive rad).
    pub gun_depression: f64,
    /// Want turret-down instead of hull-down.
    pub turret_down: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Candidate {
    pub pos: Vec2,
    pub score: f64,
    pub hull_hidden: bool,
    /// Fraction of threat observers that see the sight.
    pub coverage: f64,
}

impl Candidate {
    pub fn note(&self) -> &'static str {
        match (self.hull_hidden, self.coverage >= 0.5) {
            (true, true) => "HULL-DOWN",
            (true, false) => "TURRET-DOWN",
            (false, true) => "EXPOSED",
            (false, false) => "MASKED",
        }
    }
}

/// Observer points: two ranges × three bearings around the threat direction.
pub fn observers(q: &BpQuery, map_w: f64, map_h: f64) -> Vec<Vec2> {
    let mut out = Vec::new();
    for r in [1500.0, 2500.0] {
        for da in [-0.26, 0.0, 0.26] {
            let a = q.facing + da;
            let p = [q.centre[0] + libm::sin(a) * r, q.centre[1] + libm::cos(a) * r];
            out.push([p[0].clamp(5.0, map_w - 5.0), p[1].clamp(5.0, map_h - 5.0)]);
        }
    }
    out
}

/// Evaluate candidates. `passable(x, y)` filters ground the vehicle can occupy.
pub fn evaluate(g: &Ground, q: &BpQuery, passable: &(dyn Fn(f64, f64) -> bool + Sync)) -> Vec<Candidate> {
    let (mw, mh) = (g.map.manifest.width_m(), g.map.manifest.height_m());
    let obs = observers(q, mw, mh);
    let step = 14.0;
    let n = (2.0 * q.radius / step).ceil() as i64;
    let mut pts = Vec::new();
    for j in 0..=n {
        for i in 0..=n {
            let jx = terrain::noise::hash01(i, j, 0x6270) - 0.5;
            let jy = terrain::noise::hash01(i, j, 0x6271) - 0.5;
            let p = [
                q.centre[0] - q.radius + (i as f64 + jx * 0.8) * step,
                q.centre[1] - q.radius + (j as f64 + jy * 0.8) * step,
            ];
            let d = libm::sqrt((p[0] - q.centre[0]).powi(2) + (p[1] - q.centre[1]).powi(2));
            if d <= q.radius && p[0] > 0.0 && p[1] > 0.0 && p[0] < mw && p[1] < mh && passable(p[0], p[1]) {
                pts.push(p);
            }
        }
    }
    let eval = |p: Vec2| -> Candidate {
        let mut hull_seen = 0usize;
        let mut sight_seen = 0usize;
        for o in &obs {
            // Observer eye 2.4 m; target sample heights: hull roof and gunner's sight.
            let s = sight(g, *o, 2.4, p, &[q.hull_height * 0.95, q.sight_height]);
            if s.visible[0] {
                hull_seen += 1;
            }
            if s.visible[1] && s.transmittance > 0.3 {
                sight_seen += 1;
            }
        }
        let no = obs.len() as f64;
        let hull_hidden = hull_seen == 0;
        let coverage = sight_seen as f64 / no;
        let mut score = 0.0;
        if q.turret_down {
            score += if hull_hidden { 3.0 } else { -2.0 * hull_seen as f64 / no };
            score += 1.5 * (1.0 - coverage);
        } else {
            score += if hull_hidden && coverage >= 0.5 { 4.0 } else { 0.0 };
            score -= 2.5 * hull_seen as f64 / no;
            score += 2.0 * coverage;
        }
        // Gun depression: ground falling towards the threat steeper than the gun can depress.
        let (gx, gy) = g.map.elevation.gradient(p[0], p[1]);
        let fall = -(gx * libm::sin(q.facing) + gy * libm::cos(q.facing));
        if libm::atan(fall) > q.gun_depression * 0.8 {
            score -= 1.5;
        }
        let d = libm::sqrt((p[0] - q.centre[0]).powi(2) + (p[1] - q.centre[1]).powi(2));
        score -= 0.5 * d / q.radius;
        Candidate { pos: p, score, hull_hidden, coverage }
    };
    let mut out = par_eval(&pts, &eval);
    // Micro-search (DESIGN.md §9.2): 3 m steps around the best coarse candidates, because
    // hull-down bands behind a crest are often narrower than the coarse grid.
    let mut best: Vec<Candidate> = out.clone();
    best.sort_by(|a, b| {
        b.score.total_cmp(&a.score).then(a.pos[0].total_cmp(&b.pos[0])).then(a.pos[1].total_cmp(&b.pos[1]))
    });
    let mut seeds: Vec<Vec2> = Vec::new();
    for c in &best {
        if seeds.len() >= 32 {
            break;
        }
        if seeds.iter().all(|s| libm::sqrt((s[0] - c.pos[0]).powi(2) + (s[1] - c.pos[1]).powi(2)) > 25.0) {
            seeds.push(c.pos);
        }
    }
    let mut fine = Vec::new();
    for s in &seeds {
        for dj in -3i32..=3 {
            for di in -3i32..=3 {
                if di == 0 && dj == 0 {
                    continue;
                }
                let p = [s[0] + di as f64 * 3.0, s[1] + dj as f64 * 3.0];
                let d = libm::sqrt((p[0] - q.centre[0]).powi(2) + (p[1] - q.centre[1]).powi(2));
                if d <= q.radius && p[0] > 0.0 && p[1] > 0.0 && p[0] < mw && p[1] < mh && passable(p[0], p[1]) {
                    fine.push(p);
                }
            }
        }
    }
    out.extend(par_eval(&fine, &eval));
    out
}

/// Evaluate points on worker threads (read-only); results keep input order (deterministic).
fn par_eval(pts: &[Vec2], eval: &(dyn Fn(Vec2) -> Candidate + Sync)) -> Vec<Candidate> {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 8);
    let chunk = pts.len().div_ceil(threads).max(1);
    let mut out = Vec::with_capacity(pts.len());
    std::thread::scope(|s| {
        let handles: Vec<_> =
            pts.chunks(chunk).map(|c| s.spawn(move || c.iter().map(|&p| eval(p)).collect::<Vec<_>>())).collect();
        for h in handles {
            out.extend(h.join().expect("position worker"));
        }
    });
    out
}

/// Pick `n` positions greedily by score with minimum separation (relaxed if the area is too
/// small), then assign vehicles to positions minimising total travel distance.
pub fn assign(cands: &[Candidate], n: usize, dispersion: f64, vehicles: &[Vec2]) -> Vec<Candidate> {
    let mut sorted: Vec<&Candidate> = cands.iter().collect();
    sorted.sort_by(|a, b| {
        b.score.total_cmp(&a.score).then(a.pos[0].total_cmp(&b.pos[0])).then(a.pos[1].total_cmp(&b.pos[1]))
    });
    let mut chosen: Vec<Candidate> = Vec::new();
    // Prefer good positions with relaxed spacing over bad positions with full spacing:
    // tier 1 = within 1.0 of the best score, tier 2 = anything.
    let top = sorted.first().map_or(0.0, |c| c.score);
    for good_only in [true, false] {
        let mut sep = dispersion;
        while chosen.len() < n && sep >= dispersion * 0.35 {
            for c in &sorted {
                if chosen.len() >= n {
                    break;
                }
                if good_only && c.score < top - 1.0 {
                    continue;
                }
                if chosen
                    .iter()
                    .all(|x| libm::sqrt((x.pos[0] - c.pos[0]).powi(2) + (x.pos[1] - c.pos[1]).powi(2)) >= sep)
                {
                    chosen.push(**c);
                }
            }
            sep *= 0.8;
        }
    }
    // Brute-force assignment (n ≤ 4–5 vehicles).
    let m = chosen.len().min(vehicles.len());
    let mut best: (f64, Vec<usize>) = (f64::INFINITY, (0..m).collect());
    let mut perm: Vec<usize> = (0..chosen.len()).collect();
    permute(&mut perm, 0, &mut |p| {
        let cost: f64 = (0..m)
            .map(|v| {
                let c = chosen[p[v]].pos;
                libm::sqrt((c[0] - vehicles[v][0]).powi(2) + (c[1] - vehicles[v][1]).powi(2))
            })
            .sum();
        if cost < best.0 {
            best = (cost, p[..m].to_vec());
        }
    });
    best.1.iter().map(|&i| chosen[i]).collect()
}

fn permute(v: &mut Vec<usize>, k: usize, f: &mut dyn FnMut(&[usize])) {
    if k == v.len() {
        f(v);
        return;
    }
    for i in k..v.len() {
        v.swap(k, i);
        permute(v, k + 1, f);
        v.swap(k, i);
    }
}
