//! Called fire support (DESIGN.md decision 21): batteries are scenario assets, not map vehicles.
//!
//! Call for fire sequence (ATP 3-09.42): the observer's call → FDC delay → SHOT (rounds away) →
//! SPLASH (5 s before impact) → rounds impact → ROUNDS COMPLETE. Each volley fires one round per
//! tube with a range and deflection error that grows with range. HE bursts damage every vehicle
//! near them, friendly or not, and suppress crews; smoke rounds add a cloud.

use ai::picture::{BatteryStatus, BatteryView, Condition, ImpactView};
use rand_chacha::ChaCha8Rng;
use sim_data::{BatteryDef, FireSupportModel};
use terrain::hash::StableHasher;

use crate::command::{DenyReason, Event, FireKind, FireReport, KillKind};
use crate::picture::gauss;
use crate::sensors::uniform;
use crate::smoke::SmokeField;
use crate::world::World;
use crate::{DT, SideId, Vec2, dist, gunnery};

/// Seconds before impact that SPLASH is called.
const SPLASH_S: f64 = 5.0;

#[derive(Debug, Clone)]
pub struct Mission {
    pub at: Vec2,
    pub kind: FireKind,
    pub volleys_left: u32,
    pub next_volley_tick: u64,
    pub shot_called: bool,
}

#[derive(Debug, Clone)]
struct Flight {
    impact_tick: u64,
    at: Vec2,
    kind: FireKind,
    /// First round of a volley: it carries the SPLASH call.
    announce: bool,
    /// The last round of the mission: it carries ROUNDS COMPLETE.
    last: bool,
    splash_sent: bool,
    target: Vec2,
}

#[derive(Debug, Clone)]
pub struct Battery {
    pub side: SideId,
    pub callsign: String,
    pub def: usize,
    pub pos: Vec2,
    pub he: u32,
    pub smoke: u32,
    pub smoke_kind: usize,
    pub mission: Option<Mission>,
    /// Systematic error of this battery's fire as (range fraction, deflection rad), drawn at the
    /// first volley; observer corrections move the aim point to cancel it.
    bias: Option<(f64, f64)>,
    /// Aim point and kind of the last mission, the base for an adjustment.
    pub last_aim: Option<(Vec2, FireKind)>,
    flights: Vec<Flight>,
    /// Basic load: the stock at the start, what a resupply from the trains brings it back to.
    basic: (u32, u32),
    /// Basic loads still held in the trains.
    pub loads_left: u32,
    /// Tick at which the ammunition section is back with a basic load.
    restock_tick: Option<u64>,
    /// Missions fired from the present position.
    missions_here: u32,
    /// Tick at which a survivability move ends and the battery can fire again.
    displaced_until: Option<u64>,
}

impl Battery {
    pub fn new(side: SideId, callsign: String, def: usize, d: &BatteryDef, pos: Vec2, smoke_kind: usize) -> Self {
        Self {
            side,
            callsign,
            def,
            pos,
            he: d.he_rounds,
            smoke: d.smoke_rounds,
            smoke_kind,
            mission: None,
            bias: None,
            last_aim: None,
            flights: Vec::new(),
            basic: (d.he_rounds, d.smoke_rounds),
            loads_left: d.resupply.as_ref().map_or(0, |r| r.loads),
            restock_tick: None,
            missions_here: 0,
            displaced_until: None,
        }
    }

    /// Set the starting stock (scenario overrides); it becomes the basic load.
    pub fn set_stock(&mut self, he: u32, smoke: u32) {
        (self.he, self.smoke) = (he, smoke);
        self.basic = (he, smoke);
    }

    pub fn busy(&self) -> bool {
        self.mission.is_some() || !self.flights.is_empty()
    }

    pub fn hash_into(&self, h: &mut StableHasher) {
        h.u32(self.he);
        h.u32(self.smoke);
        h.u64(self.flights.len() as u64);
        if let Some((r, d)) = self.bias {
            h.f64(r);
            h.f64(d);
        }
        if let Some((at, _)) = self.last_aim {
            h.f64(at[0]);
            h.f64(at[1]);
        }
        if let Some(m) = &self.mission {
            h.u32(m.volleys_left);
            h.u64(m.next_volley_tick);
        }
        h.u32(self.loads_left);
        h.u64(self.restock_tick.unwrap_or(0));
        h.u32(self.missions_here);
        h.u64(self.displaced_until.unwrap_or(0));
    }

    pub fn view(&self, id: u32, d: &BatteryDef, tick: u64) -> BatteryView {
        let status = match &self.mission {
            Some(m) => BatteryStatus::Mission {
                at: m.at,
                smoke: m.kind == FireKind::Smoke,
                volleys_left: m.volleys_left,
                impact_in_s: self
                    .flights
                    .iter()
                    .map(|f| f.impact_tick)
                    .min()
                    .map(|t| t.saturating_sub(tick) as f64 * DT),
            },
            None => match self.flights.iter().min_by_key(|f| f.impact_tick) {
                Some(f) => BatteryStatus::Mission {
                    at: f.target,
                    smoke: f.kind == FireKind::Smoke,
                    volleys_left: 0,
                    impact_in_s: Some(f.impact_tick.saturating_sub(tick) as f64 * DT),
                },
                None => match self.displaced_until {
                    Some(t) => BatteryStatus::Displacing { ready_in_s: t.saturating_sub(tick) as f64 * DT },
                    None => BatteryStatus::Ready,
                },
            },
        };
        BatteryView {
            id,
            callsign: self.callsign.clone(),
            name: d.name.clone(),
            pos: self.pos,
            tubes: d.tubes,
            he_rounds: self.he,
            smoke_rounds: self.smoke,
            min_range_m: d.min_range_m.get(),
            max_range_m: d.max_range_m.get(),
            status,
            restock_in_s: self.restock_tick.map(|t| t.saturating_sub(tick) as f64 * DT),
            resupplies_left: self.loads_left,
        }
    }

    fn tof(d: &BatteryDef, range: f64) -> f64 {
        d.tof_base_s.get() + d.tof_per_km_s.get() * range / 1000.0
    }

    /// Accept or refuse a call for fire.
    pub fn call(
        &mut self,
        d: &BatteryDef,
        at: Vec2,
        kind: FireKind,
        volleys: u32,
        tick: u64,
    ) -> Result<f64, DenyReason> {
        self.start(d, at, kind, volleys, tick, d.fdc_delay_s.get())
    }

    /// Correct the last aim point by `shift` and fire again after the adjustment delay.
    pub fn adjust(&mut self, d: &BatteryDef, shift: Vec2, volleys: u32, tick: u64) -> Result<(Vec2, f64), DenyReason> {
        let Some((last, kind)) = self.last_aim else { return Err(DenyReason::NoMission) };
        if self.busy() {
            return Err(DenyReason::Busy);
        }
        let at = [last[0] + shift[0], last[1] + shift[1]];
        self.start(d, at, kind, volleys, tick, d.adjust_delay_s.get()).map(|eta| (at, eta))
    }

    fn start(
        &mut self,
        d: &BatteryDef,
        at: Vec2,
        kind: FireKind,
        volleys: u32,
        tick: u64,
        fdc: f64,
    ) -> Result<f64, DenyReason> {
        let r = dist(self.pos, at);
        if self.mission.is_some() {
            return Err(DenyReason::Busy);
        }
        if self.displaced_until.is_some() {
            return Err(DenyReason::Displacing);
        }
        if r > d.max_range_m.get() {
            return Err(DenyReason::OutOfRange);
        }
        if r < d.min_range_m.get() {
            return Err(DenyReason::TooClose);
        }
        let stock = if kind == FireKind::He { self.he } else { self.smoke };
        if stock == 0 {
            return Err(DenyReason::NoAmmo);
        }
        self.last_aim = Some((at, kind));
        self.mission = Some(Mission {
            at,
            kind,
            volleys_left: volleys.clamp(1, 6),
            next_volley_tick: tick + (fdc / DT).ceil() as u64,
            shot_called: false,
        });
        Ok(fdc + Self::tof(d, r))
    }
}

/// A burst, for the picture.
#[derive(Debug, Clone, Copy)]
pub struct Impact {
    pub tick: u64,
    pub pos: Vec2,
    pub smoke: bool,
}

pub struct Ctx<'a> {
    pub model: &'a FireSupportModel,
    pub world: &'a mut World,
    pub vehicles: &'a [sim_data::VehicleDef],
    pub smoke: &'a mut SmokeField,
    pub tick: u64,
    pub rng: &'a mut ChaCha8Rng,
    pub events: &'a mut Vec<Event>,
    pub impacts: &'a mut Vec<Impact>,
    pub infantry: &'a sim_data::InfantryModel,
    /// Per side: squad strength fraction below which it is combat-ineffective.
    pub ineffective: &'a [f64],
    pub map: &'a terrain::BakedMap,
    pub td: &'a sim_data::TerrainData,
}

/// One tick of every battery. Returns (shooter side, position) of vehicles destroyed by bursts.
pub fn step(batteries: &mut [Battery], ctx: &mut Ctx) -> Vec<(SideId, Vec2)> {
    let mut destroyed = Vec::new();
    for (bi, b) in batteries.iter_mut().enumerate() {
        let d = &ctx.model.batteries[b.def];
        let id = bi as u32;
        sustain(b, d, id, ctx);
        // Fire a volley.
        if let Some(m) = b.mission.as_mut()
            && ctx.tick >= m.next_volley_tick
        {
            let stock = if m.kind == FireKind::He { &mut b.he } else { &mut b.smoke };
            let n = (*stock).min(d.tubes);
            if n == 0 {
                ctx.events.push(Event::FireSupport {
                    side: b.side,
                    battery: id,
                    at: m.at,
                    report: FireReport::Denied(DenyReason::NoAmmo),
                });
                b.mission = None;
                continue;
            }
            *stock -= n;
            let r = dist(b.pos, m.at);
            let (ux, uy) = ((m.at[0] - b.pos[0]) / r.max(1.0), (m.at[1] - b.pos[1]) / r.max(1.0));
            let tof = Battery::tof(d, r);
            m.volleys_left -= 1;
            let last_volley = m.volleys_left == 0 || *stock == 0;
            if !m.shot_called {
                m.shot_called = true;
                b.missions_here += 1;
                ctx.events.push(Event::FireSupport { side: b.side, battery: id, at: m.at, report: FireReport::Shot });
            }
            let (bias_r, bias_d) = *b.bias.get_or_insert_with(|| {
                (gauss(ctx.rng) * d.range_bias_frac.get(), gauss(ctx.rng) * d.deflection_bias_mrad.get() / 1000.0)
            });
            for i in 0..n {
                let (er, ed) = (
                    bias_r * r + gauss(ctx.rng) * d.range_error_frac.get() * r,
                    bias_d * r + gauss(ctx.rng) * d.deflection_error_mrad.get() / 1000.0 * r,
                );
                // A spread of a few tens of metres between the guns of the battery.
                let sheaf = (i as f64 - (n as f64 - 1.0) / 2.0) * 25.0;
                let (lat, lon) = (ed + sheaf, er);
                let at = [m.at[0] + ux * lon + uy * lat, m.at[1] + uy * lon - ux * lat];
                b.flights.push(Flight {
                    impact_tick: ctx.tick + (tof / DT).ceil() as u64,
                    at,
                    kind: m.kind,
                    announce: i == 0,
                    last: last_volley && i == n - 1,
                    splash_sent: false,
                    target: m.at,
                });
            }
            if last_volley {
                b.mission = None;
            } else {
                m.next_volley_tick = ctx.tick + (d.volley_interval_s.get() / DT).ceil() as u64;
            }
        }
        // Rounds in flight.
        let tick = ctx.tick;
        let (landed, flying): (Vec<_>, Vec<_>) =
            std::mem::take(&mut b.flights).into_iter().partition(|f| f.impact_tick <= tick);
        b.flights = flying;
        for f in b.flights.iter_mut() {
            if f.announce && !f.splash_sent && (f.impact_tick - tick) as f64 * DT <= SPLASH_S {
                f.splash_sent = true;
                ctx.events.push(Event::FireSupport {
                    side: b.side,
                    battery: id,
                    at: f.target,
                    report: FireReport::Splash,
                });
            }
        }
        for f in landed {
            if f.announce && !f.splash_sent {
                ctx.events.push(Event::FireSupport {
                    side: b.side,
                    battery: id,
                    at: f.target,
                    report: FireReport::Splash,
                });
            }
            ctx.impacts.push(Impact { tick, pos: f.at, smoke: f.kind == FireKind::Smoke });
            match f.kind {
                FireKind::Smoke => ctx.smoke.add(b.smoke_kind, f.at),
                FireKind::He => destroyed.extend(burst(ctx, b.side, f.at)),
            }
            if f.last {
                ctx.events.push(Event::FireSupport {
                    side: b.side,
                    battery: id,
                    at: f.target,
                    report: FireReport::RoundsComplete,
                });
            }
        }
    }
    destroyed
}

/// Resupply from the trains and survivability moves (M8c C3, ATP 3-09.50). Both start only
/// between missions, once every round of the last one has landed.
fn sustain(b: &mut Battery, d: &BatteryDef, id: u32, ctx: &mut Ctx) {
    let tick = ctx.tick;
    let report = |b: &Battery, report| Event::FireSupport { side: b.side, battery: id, at: b.pos, report };
    if b.displaced_until.is_some_and(|t| tick >= t) {
        b.displaced_until = None;
        ctx.events.push(report(b, FireReport::InPosition));
    }
    if b.restock_tick.is_some_and(|t| tick >= t) {
        b.restock_tick = None;
        let (he, smoke) = (b.basic.0.saturating_sub(b.he), b.basic.1.saturating_sub(b.smoke));
        b.he += he;
        b.smoke += smoke;
        ctx.events.push(report(b, FireReport::Resupplied { he, smoke }));
    }
    if b.busy() {
        return;
    }
    if let Some(r) = &d.resupply
        && b.restock_tick.is_none()
        && b.loads_left > 0
        && (b.he as f64) < r.threshold_frac.get() * b.basic.0 as f64
    {
        b.loads_left -= 1;
        b.restock_tick = Some(tick + (r.resupply_s.get() / DT).ceil() as u64);
        ctx.events.push(report(b, FireReport::Restocking { ready_in_s: r.resupply_s.get() }));
    }
    if let Some(m) = &d.displace
        && b.displaced_until.is_none()
        && m.after_missions > 0
        && b.missions_here >= m.after_missions
    {
        b.missions_here = 0;
        b.displaced_until = Some(tick + (m.displace_s.get() / DT).ceil() as u64);
        ctx.events.push(report(b, FireReport::Displacing { ready_in_s: m.displace_s.get() }));
    }
}

/// An HE burst: suppression and damage to every vehicle nearby, whatever its side, and
/// casualties among squads on the ground (plan M8B-INFANTRY §4).
fn burst(ctx: &mut Ctx, side: SideId, at: Vec2) -> Vec<(SideId, Vec2)> {
    let lethal = ctx.model.effects.personnel_lethal_radius_m.get();
    if ctx.world.squad.iter().any(|s| s.is_some()) {
        crate::smallarms::he_burst(
            ctx.world,
            ctx.map,
            ctx.td,
            ctx.infantry,
            ctx.ineffective,
            at,
            lethal,
            None,
            ctx.rng,
            ctx.events,
        );
    }
    let (eff, sup) = (&ctx.model.effects, &ctx.model.suppression);
    let reach = eff.near_radius_m.get().max(sup.burst_radius_m.get());
    let mut destroyed = Vec::new();
    for k in 0..ctx.world.len() {
        if !ctx.world.alive(k) || ctx.world.mounted(k) {
            continue;
        }
        let pos = ctx.world.transform[k].pos;
        let d = dist(pos, at);
        if d > reach {
            continue;
        }
        if d < sup.burst_radius_m.get() && ctx.world.squad[k].is_none() {
            let s = &mut ctx.world.suppression[k];
            *s = (*s + sup.burst_peak.get() * (1.0 - d / sup.burst_radius_m.get())).min(1.0);
        }
        let Some(class) = ctx.world.vehicle[k].map(|vi| ctx.vehicles[vi].class) else { continue };
        let Some(ce) = ctx.model.class_effect(class) else { continue };
        let direct = d <= eff.direct_radius_m.get();
        if !direct && d > eff.near_radius_m.get() {
            continue;
        }
        let kill = gunnery::roll(if direct { &ce.direct } else { &ce.near }, 1.0, 1.0, uniform(ctx.rng));
        let sights = !direct && uniform(ctx.rng) < ce.near_sights.get();
        let kill = gunnery::apply_damage(ctx.world, k, kill, sights);
        let pen = kill.is_some_and(|kk| kk != KillKind::Crew);
        crate::infantry::carrier_hit(ctx.world, k, pen, kill, ctx.infantry, ctx.ineffective, None, ctx.rng, ctx.events);
        if let Some(kk) = kill {
            ctx.events.push(Event::Hit { unit: ctx.world.id[k], penetrated: kk != KillKind::Crew, kill });
            if ctx.world.condition[k] == Condition::Destroyed {
                destroyed.push((side, pos));
            }
        }
    }
    destroyed
}

/// Cease fire: stop the mission, rounds in the air still land.
pub fn cease(b: &mut Battery) -> bool {
    b.mission.take().is_some()
}

/// Bursts in a side's picture: within `range` of one of its units, for `window_s`.
pub fn impact_views(impacts: &[Impact], own: impl Fn(Vec2) -> bool, tick: u64, window_s: f64) -> Vec<ImpactView> {
    impacts
        .iter()
        .filter(|i| (tick - i.tick) as f64 * DT <= window_s && own(i.pos))
        .map(|i| ImpactView { pos: i.pos, time_s: i.tick as f64 * DT, smoke: i.smoke })
        .collect()
}
