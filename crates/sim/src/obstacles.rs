//! Obstacles, lanes and mines (docs/plans/M8D-ENGINEERS.md §2–§3; DESIGN.md §14 #40).
//!
//! Obstacles are a scenario layer owned by a side: a band along a polyline, rasterised once at
//! load onto the map's cells. Vehicles and squads are stopped or slowed by them in the physics
//! step (whether or not anyone knows of them); routes are planned only round the obstacles the
//! planning side knows (fog of war, decision #2), on per-side copies of the speed rasters. A lane
//! opened through an obstacle is a strip where it no longer applies (minefields: where only a
//! residual share of the mines is left).

use std::collections::BTreeMap;

use rand_chacha::ChaCha8Rng;
use sim_data::{Fuze, KitKind, ObstacleModel, ScenarioDef, VehicleDef, VehicleRule};
use terrain::{BakedMap, Grid};

use crate::{SideId, Vec2, dist};

/// A lane through an obstacle: the strip within `half` of the segment a–b.
#[derive(Debug, Clone, PartialEq)]
pub struct Lane {
    pub a: Vec2,
    pub b: Vec2,
    pub half: f64,
    /// Share of the mines left in the lane (0 = proofed clear).
    pub residual: f64,
    /// Being cut: `b` still moves on.
    pub open: bool,
}

/// Routes are planned on 4 m cells, so a vehicle routed through a lane's cells keeps within this
/// of its centre line (0.75 cell); planning and physics both use it.
pub const LANE_MIN_HALF_M: f64 = 3.0;

impl Lane {
    pub fn contains(&self, p: Vec2) -> bool {
        seg_dist(p, self.a, self.b) <= self.half.max(LANE_MIN_HALF_M)
    }
}

#[derive(Debug, Clone)]
pub struct Obstacle {
    pub side: SideId,
    /// Index into `ObstacleModel::kinds`.
    pub kind: usize,
    pub points: Vec<Vec2>,
    /// Half the band's width (m).
    pub half: f64,
    /// Ditch depth or wall height (m).
    pub height: f64,
    /// (index into `ObstacleModel::mines`, linear density in mines per metre of front).
    pub mines: Vec<(usize, f64)>,
    /// Known to each side.
    pub known: Vec<bool>,
    pub lanes: Vec<Lane>,
    /// Map cells it covers.
    pub cells: Vec<(u32, u32)>,
}

impl Obstacle {
    /// Is `p` in one of its lanes (or in the band at all: see `ObstacleField::at`)?
    pub fn lane_at(&self, p: Vec2) -> Option<&Lane> {
        self.lanes.iter().find(|l| l.contains(p))
    }

    /// Distance from `p` to the obstacle's line.
    pub fn distance(&self, p: Vec2) -> f64 {
        self.points.windows(2).map(|w| seg_dist(p, w[0], w[1])).fold(f64::INFINITY, f64::min)
    }

    /// Closest point of the obstacle's line to `p`.
    pub fn nearest(&self, p: Vec2) -> Vec2 {
        let mut best = (f64::INFINITY, self.points[0]);
        for w in self.points.windows(2) {
            let q = seg_closest(p, w[0], w[1]);
            let d = dist(p, q);
            if d < best.0 {
                best = (d, q);
            }
        }
        best.1
    }
}

/// A breaching kit fitted to a vehicle.
#[derive(Debug, Clone, PartialEq)]
pub struct KitState {
    /// Index into `ObstacleModel::kits`.
    pub kit: usize,
    /// Mine strikes it has taken (rollers wear out).
    pub strikes: u32,
    pub wrecked: bool,
}

/// What a body moving through an obstacle cell meets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Passage {
    Clear,
    /// Speed factor (0..1).
    Slow(f64),
    Blocked,
}

#[derive(Debug, Clone)]
pub struct ObstacleField {
    pub list: Vec<Obstacle>,
    /// Cell → obstacle index + 1 (0 = none; where bands overlap the first listed wins).
    pub raster: Grid<u16>,
    pub cell_m: f64,
    /// Fitted kits by entity index.
    pub kits: BTreeMap<usize, KitState>,
    /// Per side: knowledge or lanes changed, so its planning rasters need rebuilding.
    pub dirty: Vec<bool>,
    /// Entity → the obstacle it is stopped at (one `ObstacleMet` per stop; not hashed).
    pub met: BTreeMap<usize, usize>,
}

/// Distance from `p` to segment a–b.
pub fn seg_dist(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    dist(p, seg_closest(p, a, b))
}

fn seg_closest(p: Vec2, a: Vec2, b: Vec2) -> Vec2 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    [a[0] + t * dx, a[1] + t * dy]
}

/// Cells within `half` of the polyline (at least 0.75 cell, so a thin band stays closed to
/// diagonal moves), in row order.
fn band_cells(points: &[Vec2], half: f64, cell: f64, w: usize, h: usize) -> Vec<(u32, u32)> {
    let r = half.max(cell * 0.75);
    let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in points {
        x0 = x0.min(p[0]);
        y0 = y0.min(p[1]);
        x1 = x1.max(p[0]);
        y1 = y1.max(p[1]);
    }
    let lo = |v: f64| (((v - r) / cell).floor().max(0.0)) as usize;
    let (i0, j0) = (lo(x0), lo(y0));
    let i1 = (((x1 + r) / cell).ceil() as usize).min(w);
    let j1 = (((y1 + r) / cell).ceil() as usize).min(h);
    let mut out = Vec::new();
    for j in j0..j1 {
        for i in i0..i1 {
            let c = [(i as f64 + 0.5) * cell, (j as f64 + 0.5) * cell];
            if points.windows(2).any(|s| seg_dist(c, s[0], s[1]) <= r) {
                out.push((i as u32, j as u32));
            }
        }
    }
    out
}

impl ObstacleField {
    /// The scenario's obstacles on `map`; `kits` = (entity, kit id) for fitted units.
    pub fn new(
        scenario: &ScenarioDef,
        model: &ObstacleModel,
        map: &BakedMap,
        kits: &[(usize, String)],
    ) -> Result<Self, String> {
        let n_sides = scenario.sides.len();
        let (w, h) = (map.landcover.w, map.landcover.h);
        let cell = map.manifest.cell_size_m;
        let mut list = Vec::new();
        for (si, s) in scenario.sides.iter().enumerate() {
            for (oi, o) in s.obstacles.iter().enumerate() {
                let err = |m: String| format!("side {} obstacle {}: {m}", s.name, oi + 1);
                let kind = model.kind_index(&o.kind).ok_or_else(|| err(format!("unknown kind '{}'", o.kind)))?;
                let kd = &model.kinds[kind];
                if o.points.len() < 2 {
                    return Err(err("needs at least two points".into()));
                }
                let width = o.width_m.unwrap_or(kd.width_m.get());
                if !(width.is_finite() && width > 0.0) {
                    return Err(err(format!("bad width {width}")));
                }
                let mut mines = Vec::new();
                for (id, density) in &o.mines {
                    let mi = model.mine_index(id).ok_or_else(|| err(format!("unknown mine '{id}'")))?;
                    if !(density.is_finite() && *density > 0.0) {
                        return Err(err(format!("bad mine density {density}")));
                    }
                    mines.push((mi, *density));
                }
                if kd.vehicle == VehicleRule::Mined && mines.is_empty() {
                    return Err(err("a minefield needs mines".into()));
                }
                let points: Vec<Vec2> = o.points.clone();
                let half = width / 2.0;
                let cells = band_cells(&points, half, cell, w, h);
                let mut known = vec![false; n_sides];
                known[si] = true;
                list.push(Obstacle {
                    side: SideId(si as u8),
                    kind,
                    points,
                    half,
                    height: o.height_m.unwrap_or(kd.height_m.get()),
                    mines,
                    known,
                    lanes: Vec::new(),
                    cells,
                });
            }
        }
        if list.len() >= u16::MAX as usize {
            return Err("too many obstacles".into());
        }
        let mut raster = Grid::new(w, h, 0u16);
        for (n, o) in list.iter().enumerate() {
            for &(i, j) in &o.cells {
                if raster.get(i as usize, j as usize) == 0 {
                    raster.set(i as usize, j as usize, n as u16 + 1);
                }
            }
        }
        let mut fitted = BTreeMap::new();
        for (k, id) in kits {
            let kit = model.kit_index(id).ok_or_else(|| format!("unknown kit '{id}'"))?;
            fitted.insert(*k, KitState { kit, strikes: 0, wrecked: false });
        }
        let dirty = (0..n_sides).map(|s| list.iter().any(|o| o.known[s])).collect();
        Ok(Self { list, raster, cell_m: cell, kits: fitted, dirty, met: BTreeMap::new() })
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty() && self.kits.is_empty()
    }

    /// The obstacle whose band covers `p`.
    pub fn at(&self, p: Vec2) -> Option<usize> {
        let (i, j) = ((p[0] / self.cell_m).floor(), (p[1] / self.cell_m).floor());
        if i < 0.0 || j < 0.0 || i as usize >= self.raster.w || j as usize >= self.raster.h {
            return None;
        }
        match self.raster.get(i as usize, j as usize) {
            0 => None,
            n => Some(n as usize - 1),
        }
    }

    /// What obstacle `o` does to vehicle `v` at `p`.
    pub fn vehicle_passage(&self, model: &ObstacleModel, o: usize, v: &VehicleDef, p: Vec2) -> Passage {
        let ob = &self.list[o];
        if ob.lane_at(p).is_some() {
            return Passage::Clear;
        }
        vehicle_rule(model, ob, v)
    }

    /// What obstacle `o` does to a man on foot at `p`.
    pub fn foot_passage(&self, model: &ObstacleModel, o: usize, p: Vec2) -> Passage {
        let ob = &self.list[o];
        if ob.lane_at(p).is_some() {
            return Passage::Clear;
        }
        foot_rule(model, ob)
    }

    /// Write the obstacles `side` knows into a copy of a speed raster: `vehicle` None = foot.
    pub fn apply(&self, model: &ObstacleModel, side: SideId, vehicle: Option<&VehicleDef>, grid: &mut Grid<u8>) {
        for ob in self.list.iter().filter(|o| o.known[side.0 as usize]) {
            let rule = match vehicle {
                Some(v) => match vehicle_rule(model, ob, v) {
                    // Planners treat a known minefield as closed: a route goes round or through a
                    // lane (decision #3: BYPASS by default).
                    Passage::Clear if model.kinds[ob.kind].vehicle == VehicleRule::Mined => Passage::Blocked,
                    p => p,
                },
                None => foot_rule(model, ob),
            };
            if rule == Passage::Clear {
                continue;
            }
            for &(i, j) in &ob.cells {
                let c = [(i as f64 + 0.5) * self.cell_m, (j as f64 + 0.5) * self.cell_m];
                if ob.lanes.iter().any(|l| l.contains(c)) {
                    continue;
                }
                let (i, j) = (i as usize, j as usize);
                let v = grid.get(i, j);
                grid.set(
                    i,
                    j,
                    match rule {
                        Passage::Blocked => 0,
                        Passage::Slow(f) if v > 0 => ((v as f64 * f).round() as u8).max(1),
                        _ => v,
                    },
                );
            }
        }
    }

    /// Mark obstacle `o` known to `side`. True if it was news.
    pub fn learn(&mut self, o: usize, side: SideId) -> bool {
        let s = side.0 as usize;
        if self.list[o].known[s] {
            return false;
        }
        self.list[o].known[s] = true;
        self.dirty[s] = true;
        true
    }

    /// Every side that knows obstacle `o` must re-plan on it (a lane changed).
    pub fn touch(&mut self, o: usize) {
        for (s, k) in self.list[o].known.iter().enumerate() {
            if *k {
                self.dirty[s] = true;
            }
        }
    }

    pub fn hash_into(&self, h: &mut terrain::hash::StableHasher) {
        for o in &self.list {
            for k in &o.known {
                h.u8(*k as u8);
            }
            h.u32(o.lanes.len() as u32);
            for l in &o.lanes {
                for v in [l.a[0], l.a[1], l.b[0], l.b[1], l.residual] {
                    h.f64(v);
                }
                h.u8(l.open as u8);
            }
        }
        for (k, s) in &self.kits {
            h.u64(*k as u64);
            h.u32(s.strikes);
            h.u8(s.wrecked as u8);
        }
    }
}

fn vehicle_rule(model: &ObstacleModel, ob: &Obstacle, v: &VehicleDef) -> Passage {
    match model.kinds[ob.kind].vehicle {
        VehicleRule::NoGo => Passage::Blocked,
        VehicleRule::Slow(f) => Passage::Slow(f),
        VehicleRule::Mined => Passage::Clear,
        VehicleRule::Gap => {
            let m = &v.mobility;
            if ob.half * 2.0 <= m.trench_width_m.get() && ob.height <= m.vertical_step_m.get() {
                Passage::Clear
            } else {
                Passage::Blocked
            }
        }
    }
}

fn foot_rule(model: &ObstacleModel, ob: &Obstacle) -> Passage {
    match model.kinds[ob.kind].foot_factor.get() {
        f if f <= 0.0 => Passage::Blocked,
        f if f >= 1.0 => Passage::Clear,
        f => Passage::Slow(f),
    }
}

/// Mine strike hazard per metre travelled in minefield `ob` for a vehicle with `kit`, with
/// `residual` of the mines left (1 outside a lane): the expected strikes over the field's depth
/// are linear density × effective width (FM 20-32 ch. 2–3; `sim_data::encounter_probability`), so
/// per metre of travel they are that over the depth. Returns per mine type (index, rate, the share
/// a kit clears).
pub fn mine_rates(
    model: &ObstacleModel,
    ob: &Obstacle,
    kit: Option<&KitState>,
    residual: f64,
) -> Vec<(usize, f64, f64)> {
    let depth = (ob.half * 2.0).max(1.0);
    ob.mines
        .iter()
        .map(|&(mi, density)| {
            let m = &model.mines[mi];
            let cleared = kit.filter(|k| !k.wrecked).map_or(0.0, |k| {
                let kd = &model.kits[k.kit];
                match m.fuze {
                    Fuze::Pressure => kd.clear_pressure.get(),
                    Fuze::FullWidth => kd.clear_full_width.get(),
                }
            });
            (mi, density * m.effective_width_m.get() / depth * residual, cleared)
        })
        .collect()
}

/// Outcome of a mine going off under or ahead of a vehicle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strike {
    /// The kit took it (roller) or pushed it aside (plough): no harm.
    Kit,
    Destroyed,
    Immobilised,
    NoEffect,
}

/// Roll for strikes over `ds` metres (one draw per call, then the type and effect).
pub fn roll_strike(
    model: &ObstacleModel,
    rates: &[(usize, f64, f64)],
    kit: Option<&mut KitState>,
    ds: f64,
    rng: &mut ChaCha8Rng,
) -> Option<Strike> {
    let total: f64 = rates.iter().map(|r| r.1).sum();
    if total <= 0.0 || ds <= 0.0 {
        return None;
    }
    let u = crate::sensors::uniform(rng);
    if u >= 1.0 - (-total * ds).exp() {
        return None;
    }
    // Which mine type: by its share of the rate.
    let mut pick = crate::sensors::uniform(rng) * total;
    let mut chosen = rates[rates.len() - 1];
    for r in rates {
        if pick < r.1 {
            chosen = *r;
            break;
        }
        pick -= r.1;
    }
    let (mi, _, cleared) = chosen;
    if let Some(k) = kit
        && !k.wrecked
        && crate::sensors::uniform(rng) < cleared
    {
        if model.kits[k.kit].kind == KitKind::Roller {
            k.strikes += 1;
            if model.kits[k.kit].strikes.as_ref().is_some_and(|s| k.strikes as f64 >= s.get()) {
                k.wrecked = true;
            }
        }
        return Some(Strike::Kit);
    }
    let m = &model.mines[mi];
    let e = crate::sensors::uniform(rng);
    Some(if e < m.destroy_share.get() {
        Strike::Destroyed
    } else if e < m.destroy_share.get() + m.immobilise_share.get() {
        Strike::Immobilised
    } else {
        Strike::NoEffect
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_diagonal_band_has_no_diagonal_gap() {
        // 4 m cells, a thin band at 45 degrees: every step across it meets a band cell.
        let cells = band_cells(&[[2.0, 2.0], [98.0, 98.0]], 1.0, 4.0, 30, 30);
        let set: std::collections::BTreeSet<(u32, u32)> = cells.into_iter().collect();
        for i in 1..20u32 {
            // Crossing diagonally from (i+1, i-1) to (i-1, i+1) passes (i, i) and its orthogonal
            // neighbours: at least two of the three are in the band.
            let n = [(i, i), (i + 1, i), (i, i + 1)].iter().filter(|c| set.contains(c)).count();
            assert!(n >= 2, "gap at {i}: {n}");
        }
    }

    #[test]
    fn strikes_follow_the_expected_rate() {
        use rand_chacha::rand_core::SeedableRng;
        // One mine type, 1 strike expected over the field: about 63 % of crossings strike.
        let rates = [(0usize, 1.0 / 100.0, 0.0)];
        let model = sim_data::DataSet::load(&std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data"))
            .unwrap()
            .obstacles;
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let n = 4000;
        let mut hit = 0;
        for _ in 0..n {
            let mut struck = false;
            for _ in 0..100 {
                if roll_strike(&model, &rates, None, 1.0, &mut rng).is_some() {
                    struck = true;
                    break;
                }
            }
            hit += struck as u32;
        }
        let p = hit as f64 / n as f64;
        assert!((p - 0.632).abs() < 0.03, "{p:.3}");
    }
}

/// Planning rasters with the obstacles each side knows (decision #2), per (side, class id) where
/// the class is a vehicle id or `foot`: `plain` from the class's own raster, `avoid` from the
/// built-up avoiding one (`builtup`). A side that knows no obstacle has none and plans on the
/// shared rasters.
#[derive(Debug, Default)]
pub struct Layers {
    plain: BTreeMap<(u8, String), Grid<u8>>,
    avoid: BTreeMap<(u8, String), Grid<u8>>,
}

impl Layers {
    /// The raster `side` plans class `id`'s routes on.
    pub fn speed<'a>(
        &'a self,
        map: &'a BakedMap,
        avoid_layers: &'a crate::builtup::AvoidLayers,
        side: u8,
        id: &str,
        avoid: bool,
    ) -> Option<&'a Grid<u8>> {
        let key = (side, id.to_string());
        if avoid && let Some(g) = self.avoid.get(&key).or_else(|| avoid_layers.get(&key)) {
            return Some(g);
        }
        if let Some(g) = self.plain.get(&key) {
            return Some(g);
        }
        Some(&map.mobility(id)?.speed)
    }

    /// Does `side` know any obstacle (has rasters of its own)?
    pub fn knows(&self, side: u8) -> bool {
        self.plain.keys().any(|k| k.0 == side)
    }

    /// The foot raster `side` plans squad routes on.
    pub fn foot<'a>(&'a self, map: &'a BakedMap, side: u8) -> Option<&'a Grid<u8>> {
        let id = terrain::mobility::FOOT_CLASS;
        self.plain.get(&(side, id.to_string())).or_else(|| map.mobility(id).map(|l| &l.speed))
    }

    /// Rebuild `side`'s rasters for the classes `ids` from what it knows now.
    #[allow(clippy::too_many_arguments)]
    pub fn rebuild(
        &mut self,
        field: &ObstacleField,
        model: &ObstacleModel,
        map: &BakedMap,
        avoid_layers: &crate::builtup::AvoidLayers,
        vehicles: &[VehicleDef],
        side: SideId,
        ids: &[String],
    ) {
        self.plain.retain(|k, _| k.0 != side.0);
        self.avoid.retain(|k, _| k.0 != side.0);
        if !field.list.iter().any(|o| o.known[side.0 as usize]) {
            return;
        }
        for id in ids {
            let v = vehicles.iter().find(|v| &v.id == id);
            let Some(base) = map.mobility(id) else { continue };
            let mut g = base.speed.clone();
            field.apply(model, side, v, &mut g);
            self.plain.insert((side.0, id.clone()), g);
            if let Some(a) = avoid_layers.get(&(side.0, id.clone())) {
                let mut g = a.clone();
                field.apply(model, side, v, &mut g);
                self.avoid.insert((side.0, id.clone()), g);
            }
        }
    }
}
