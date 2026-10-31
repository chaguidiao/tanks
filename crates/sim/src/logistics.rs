//! Service-station resupply at supply points (docs/plans/M8C-LOGISTICS.md; FM 17-15 (1996) and
//! ATP 3-20.15 (2025) on tank platoon sustainment: in the service-station method vehicles leave
//! their positions a few at a time, pull into the resupply point, take on fuel and ammunition and
//! return). Transfers run at the rates in `data/combat/logistics.ron`, in whole rounds, with no
//! randomness; stocks are conserved between the point and the vehicles.

use ai::picture::{ServiceStage, SideId};
use sim_data::{AmmoDef, LogisticsModel, StowClass, VehicleDef};
use terrain::hash::StableHasher;

use crate::Vec2;
use crate::world::World;

/// Fuel, rounds and smoke salvos held by a supply point or carried by a supply truck.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Stock {
    pub unlimited: bool,
    pub fuel_l: f64,
    /// Rounds per index of the sim's ammo table.
    pub rounds: Vec<u32>,
    pub smoke_salvos: u32,
}

impl Stock {
    fn has_rounds(&self, ammo: usize) -> bool {
        self.unlimited || self.rounds.get(ammo).is_some_and(|&n| n > 0)
    }

    fn take_round(&mut self, ammo: usize) {
        if !self.unlimited {
            self.rounds[ammo] -= 1;
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.unlimited && self.fuel_l <= 1e-6 && self.smoke_salvos == 0 && self.rounds.iter().all(|&n| n == 0)
    }

    pub(crate) fn hash_into(&self, h: &mut StableHasher) {
        h.f64(self.fuel_l);
        h.u32(self.smoke_salvos);
        for &n in &self.rounds {
            h.u32(n);
        }
    }
}

/// A side's supply point (a scenario item, not an entity).
#[derive(Debug, Clone, PartialEq)]
pub struct SupplyPoint {
    pub side: SideId,
    pub callsign: String,
    pub pos: Vec2,
    pub stock: Stock,
}

impl SupplyPoint {
    pub(crate) fn hash_into(&self, h: &mut StableHasher) {
        self.stock.hash_into(h);
    }
}

/// Default cargo of a supply truck (plan M8C-TAILGATE-TRUCKS §3): the fuel and smoke salvos of
/// its type, and for each stow class its round capacity shared out over the rounds of that class
/// the side's vehicles carry, in proportion to how many they carry. Whole rounds; what rounding
/// leaves over goes to the most carried round. `carried` is rounds per ammo index; `n_ammo` the
/// length of the ammo table.
pub fn default_cargo(c: &sim_data::CargoDef, carried: &[u32], ammo: &[AmmoDef]) -> Stock {
    let mut rounds = vec![0u32; ammo.len()];
    for (class, cap) in [
        (StowClass::MainGun, c.main_gun_rounds),
        (StowClass::Autocannon, c.autocannon_rounds),
        (StowClass::Missile, c.missiles),
    ] {
        let idx: Vec<usize> = (0..ammo.len())
            .filter(|&i| carried.get(i).is_some_and(|&n| n > 0) && ammo[i].stow_class() == class)
            .collect();
        let total: u64 = idx.iter().map(|&i| carried[i] as u64).sum();
        if total == 0 || cap == 0 {
            continue;
        }
        let mut given = 0;
        for &i in &idx {
            let n = (cap as u64 * carried[i] as u64 / total) as u32;
            rounds[i] = n;
            given += n;
        }
        // Ties go to the lower ammo index (stable, deterministic).
        let top = *idx.iter().max_by(|&&a, &&b| carried[a].cmp(&carried[b]).then(b.cmp(&a))).expect("non-empty");
        rounds[top] += cap - given;
    }
    Stock { unlimited: false, fuel_l: c.fuel_l.get(), rounds, smoke_salvos: c.smoke_salvos }
}

/// Transfer credit classes: rounds by stow class, then smoke salvos.
const SMOKE: usize = 3;

fn class_index(c: StowClass) -> usize {
    match c {
        StowClass::MainGun => 0,
        StowClass::Autocannon => 1,
        StowClass::Missile => 2,
    }
}

/// One platoon member's resupply state.
#[derive(Debug, Clone, PartialEq)]
pub struct ServiceState {
    /// Full, or nothing more it needs is stocked at the point.
    pub done: bool,
    /// Finished because the point ran short of something it still needed.
    pub short: bool,
    /// Fractional rounds/salvos earned but not yet handed over, per class (main gun, autocannon,
    /// missile, smoke).
    pub credit: [f64; 4],
    /// Where the member was last routed (NaN = not yet).
    pub goal: Vec2,
    /// For the picture only (follows from the fields above and positions; not hashed).
    pub stage: ServiceStage,
}

impl ServiceState {
    pub fn new() -> Self {
        Self { done: false, short: false, credit: [0.0; 4], goal: [f64::NAN, f64::NAN], stage: ServiceStage::ToPoint }
    }

    pub(crate) fn hash_into(&self, h: &mut StableHasher) {
        h.u8(self.done as u8 | (self.short as u8) << 1);
        for c in self.credit {
            h.f64(c);
        }
    }
}

impl Default for ServiceState {
    fn default() -> Self {
        Self::new()
    }
}

/// Read-only tables a transfer needs.
pub struct Tables<'a> {
    pub vehicles: &'a [VehicleDef],
    /// Per vehicle type: the ammo-table index of each loadout entry.
    pub loadouts: &'a [Vec<usize>],
    pub ammo: &'a [AmmoDef],
    pub model: &'a LogisticsModel,
}

/// Serve vehicle `k` from `point` (a supply point's or a truck's stock) for `dt` seconds. Returns
/// true when it is done: full in every item, or the stock has nothing more of what it lacks.
pub fn transfer(world: &mut World, k: usize, point: &mut Stock, st: &mut ServiceState, t: &Tables, dt: f64) -> bool {
    let Some(vi) = world.vehicle[k] else { return true };
    let def = &t.vehicles[vi];
    let mut wants = false;

    // Fuel.
    if let Some(m) = world.mobility[k].as_mut() {
        let need = (def.mobility.fuel_capacity_l.get() - m.fuel_l).max(0.0);
        let avail = if point.unlimited { f64::INFINITY } else { point.fuel_l };
        let give = need.min(t.model.fuel_l_per_min.get() / 60.0 * dt).min(avail);
        if give > 0.0 {
            m.fuel_l += give;
            if !point.unlimited {
                point.fuel_l -= give;
            }
        }
        wants |= need - give > 1e-6 && avail - give > 1e-6;
    }

    // Rounds, by stow class, loadout entries in order.
    if let Some(g) = world.gun[k].as_mut() {
        let lo = &t.loadouts[vi];
        for class in [StowClass::MainGun, StowClass::Autocannon, StowClass::Missile] {
            let c = class_index(class);
            let open = |g: &crate::gunnery::GunState, point: &Stock, i: usize| {
                g.stock[i] < g.capacity[i] && t.ammo[lo[i]].stow_class() == class && point.has_rounds(lo[i])
            };
            if !(0..lo.len()).any(|i| open(g, point, i)) {
                st.credit[c] = 0.0;
                continue;
            }
            st.credit[c] += t.model.rounds_per_min(class) / 60.0 * dt;
            while st.credit[c] >= 1.0 {
                let Some(i) = (0..lo.len()).find(|&i| open(g, point, i)) else { break };
                g.stock[i] += 1;
                point.take_round(lo[i]);
                st.credit[c] -= 1.0;
            }
            if (0..lo.len()).any(|i| open(g, point, i)) {
                wants = true;
            } else {
                st.credit[c] = 0.0;
            }
        }
    }

    // Smoke grenade salvos.
    let cap = def.smoke_launcher.as_ref().map_or(0, |l| l.salvos);
    let sm = &mut world.smoke[k];
    let has = |p: &Stock| p.unlimited || p.smoke_salvos > 0;
    if sm.salvos < cap && has(point) {
        st.credit[SMOKE] += t.model.smoke_salvos_per_min.get() / 60.0 * dt;
        while st.credit[SMOKE] >= 1.0 && sm.salvos < cap && has(point) {
            sm.salvos += 1;
            if !point.unlimited {
                point.smoke_salvos -= 1;
            }
            st.credit[SMOKE] -= 1.0;
        }
        wants |= sm.salvos < cap && has(point);
    }
    if !wants {
        st.credit[SMOKE] = 0.0;
    }
    !wants
}

/// Whether vehicle `k` lacks anything (fuel, a carried round type, smoke salvos).
pub fn needs_anything(world: &World, k: usize, t: &Tables) -> bool {
    let Some(vi) = world.vehicle[k] else { return false };
    let def = &t.vehicles[vi];
    let fuel = world.mobility[k].as_ref().is_some_and(|m| m.fuel_l < def.mobility.fuel_capacity_l.get() - 1e-6);
    let rounds = world.gun[k].as_ref().is_some_and(|g| g.stock.iter().zip(&g.capacity).any(|(s, c)| s < c));
    let smoke = world.smoke[k].salvos < def.smoke_launcher.as_ref().map_or(0, |l| l.salvos);
    fuel || rounds || smoke
}

/// Off-map LOGPAC (scenario re-arm event): add `fraction` of the full load of every round type
/// and of the smoke salvos to vehicle `k`, in whole rounds rounded down, capped at the full load.
pub fn rearm(world: &mut World, k: usize, def: &VehicleDef, fraction: f64) {
    if let Some(g) = world.gun[k].as_mut() {
        for (s, &c) in g.stock.iter_mut().zip(&g.capacity) {
            *s = (*s + (c as f64 * fraction) as u32).min(c);
        }
    }
    let cap = def.smoke_launcher.as_ref().map_or(0, |l| l.salvos);
    let sm = &mut world.smoke[k];
    sm.salvos = (sm.salvos + (cap as f64 * fraction) as u32).min(cap);
}

/// Off-map LOGPAC: add `fraction` of the fuel capacity to vehicle `k`, capped at a full tank.
pub fn refuel(world: &mut World, k: usize, def: &VehicleDef, fraction: f64) {
    if let Some(m) = world.mobility[k].as_mut() {
        let cap = def.mobility.fuel_capacity_l.get();
        m.fuel_l = (m.fuel_l + cap * fraction).min(cap.max(m.fuel_l));
    }
}

/// Supply point of `side` with index `id` among all points, if it belongs to that side.
pub fn point_of(points: &[SupplyPoint], side: SideId, id: u32) -> Option<usize> {
    let i = id as usize;
    (points.get(i)?.side == side).then_some(i)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> sim_data::DataSet {
        sim_data::DataSet::load(std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data"))).unwrap()
    }

    /// An M1A1-class tank (1990: the early sabot is out of service, so it carries none of it)
    /// that has fired 10 sabot rounds, used 500 l and both smoke salvos.
    fn depleted(ds: &sim_data::DataSet) -> (World, Vec<VehicleDef>, Vec<Vec<usize>>, Vec<AmmoDef>) {
        let def = ds.vehicle("bulwark_a1").unwrap().clone();
        let ammo: Vec<AmmoDef> = def.armament.load.iter().map(|l| ds.ammo(&l.ammo).unwrap().clone()).collect();
        let lo: Vec<usize> = (0..ammo.len()).collect();
        let mut w = World::default();
        w.spawn_vehicle(SideId(0), "1-1", 0, &def, [0.0, 0.0], 0.0, 0.0, 1990, vec![0, 24, 12, 4]);
        w.gun[0].as_mut().unwrap().stock[1] = 14;
        w.mobility[0].as_mut().unwrap().fuel_l -= 500.0;
        w.smoke[0].salvos = 0;
        (w, vec![def], vec![lo], ammo)
    }

    fn point(n_ammo: usize, sabot: u32) -> Stock {
        let mut rounds = vec![0; n_ammo];
        rounds[1] = sabot;
        Stock { unlimited: false, fuel_l: 10_000.0, rounds, smoke_salvos: 1 }
    }

    /// Plan M8C-TAILGATE-TRUCKS §3: a truck's round capacity is shared over the rounds its side
    /// carries, by stow class and in proportion, in whole rounds that add up to the capacity.
    #[test]
    fn default_cargo_shares_the_capacity_over_the_sides_rounds() {
        let ds = data();
        let (_, _, _, ammo) = depleted(&ds);
        let c = ds.vehicle("carter_c").unwrap().cargo.clone().unwrap();
        // The tank's 1990 load: 24 of one main-gun round, 12 of another, 4 of a third (index 3).
        let carried = vec![0, 24, 12, 4];
        let st = default_cargo(&c, &carried, &ammo);
        let main: u32 =
            (0..ammo.len()).filter(|&i| ammo[i].stow_class() == StowClass::MainGun).map(|i| st.rounds[i]).sum();
        assert_eq!(main, c.main_gun_rounds, "the whole main-gun capacity is used");
        assert_eq!(st.rounds[0], 0, "a round nobody carries is not loaded");
        assert!(st.rounds[1] > st.rounds[2] && st.rounds[2] >= st.rounds[3], "{:?}", st.rounds);
        assert_eq!((st.fuel_l, st.smoke_salvos), (c.fuel_l.get(), c.smoke_salvos));
        assert!(default_cargo(&c, &[0, 0, 0, 0], &ammo).rounds.iter().all(|&n| n == 0));
    }

    #[test]
    fn transfer_conserves_stock_and_runs_at_the_data_rates() {
        let ds = data();
        let (mut w, vehicles, loadouts, ammo) = depleted(&ds);
        let t = Tables { vehicles: &vehicles, loadouts: &loadouts, ammo: &ammo, model: &ds.logistics };
        let mut sp = point(ammo.len(), 100);
        let mut st = ServiceState::new();
        let rate = ds.logistics.main_gun_rounds_per_min.get();
        let mut done_at = None;
        for s in 1..=900 {
            if transfer(&mut w, 0, &mut sp, &mut st, &t, 1.0) && done_at.is_none() {
                done_at = Some(s);
            }
            if s == 60 {
                let g = w.gun[0].as_ref().unwrap();
                assert_eq!(g.stock[1], 14 + rate as u32, "one minute at {rate} rounds a minute");
            }
        }
        let g = w.gun[0].as_ref().unwrap();
        assert_eq!(g.stock, vec![0, 24, 12, 4], "filled to capacity; the out-of-service round stays at 0");
        assert_eq!(sp.rounds[1], 90, "the point lost exactly what the tank took");
        let cap = vehicles[0].mobility.fuel_capacity_l.get();
        assert!((w.mobility[0].as_ref().unwrap().fuel_l - cap).abs() < 1e-9);
        assert!((sp.fuel_l - 9_500.0).abs() < 1e-9, "fuel conserved: {}", sp.fuel_l);
        assert_eq!((w.smoke[0].salvos, sp.smoke_salvos), (1, 0), "one salvo stocked, one given");
        // 10 rounds take 200 s at 3 a minute; fuel and smoke finish earlier.
        let expect = (10.0 / rate * 60.0).ceil() as u32;
        assert_eq!(done_at, Some(expect));
        assert!(!needs_anything(&w, 0, &t) || w.smoke[0].salvos < 2, "only the smoke the point lacked is missing");
    }

    #[test]
    fn a_short_point_gives_what_it_has_and_reports_done() {
        let ds = data();
        let (mut w, vehicles, loadouts, ammo) = depleted(&ds);
        let t = Tables { vehicles: &vehicles, loadouts: &loadouts, ammo: &ammo, model: &ds.logistics };
        let mut sp = point(ammo.len(), 4);
        sp.fuel_l = 100.0;
        let mut st = ServiceState::new();
        let mut done = false;
        for _ in 0..600 {
            done = transfer(&mut w, 0, &mut sp, &mut st, &t, 1.0);
            if done {
                break;
            }
        }
        assert!(done, "done once the point has nothing more the tank lacks");
        assert_eq!(w.gun[0].as_ref().unwrap().stock[1], 18);
        assert_eq!((sp.rounds[1], sp.fuel_l), (0, 0.0));
        assert!(needs_anything(&w, 0, &t));
    }
}
