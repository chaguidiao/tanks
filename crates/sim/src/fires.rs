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
        }
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
                None => BatteryStatus::Ready,
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
}

/// One tick of every battery. Returns (shooter side, position) of vehicles destroyed by bursts.
pub fn step(batteries: &mut [Battery], ctx: &mut Ctx) -> Vec<(SideId, Vec2)> {
    let mut destroyed = Vec::new();
    for (bi, b) in batteries.iter_mut().enumerate() {
        let d = &ctx.model.batteries[b.def];
        let id = bi as u32;
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

/// An HE burst: suppression and damage to every vehicle nearby, whatever its side.
fn burst(ctx: &mut Ctx, side: SideId, at: Vec2) -> Vec<(SideId, Vec2)> {
    let (eff, sup) = (&ctx.model.effects, &ctx.model.suppression);
    let reach = eff.near_radius_m.get().max(sup.burst_radius_m.get());
    let mut destroyed = Vec::new();
    for k in 0..ctx.world.len() {
        if !ctx.world.alive(k) {
            continue;
        }
        let pos = ctx.world.transform[k].pos;
        let d = dist(pos, at);
        if d > reach {
            continue;
        }
        if d < sup.burst_radius_m.get() {
            let s = &mut ctx.world.suppression[k];
            *s = (*s + sup.burst_peak.get() * (1.0 - d / sup.burst_radius_m.get())).min(1.0);
        }
        let Some(class) = ctx.world.vehicle[k].map(|vi| ctx.vehicles[vi].class) else { continue };
        let Some(ce) = ctx.model.class_effect(class) else { continue };
        let direct = d <= eff.direct_radius_m.get();
        if !direct && d > eff.near_radius_m.get() {
            continue;
        }
        let kill = gunnery::roll(if direct { &ce.direct } else { &ce.near }, 1.0, uniform(ctx.rng));
        let sights = !direct && uniform(ctx.rng) < ce.near_sights.get();
        let kill = gunnery::apply_damage(ctx.world, k, kill, sights);
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
