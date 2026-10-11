//! Sensors and detection (DESIGN.md §6.1), run at 4 Hz.
//!
//! For each observer/target pair with line of sight:
//! - critical dimension `d_c = sqrt(projected width × visible height)` from the target aspect
//!   and the LOS visible fraction;
//! - resolvable cycles `N = d_c / R · 1000 · cycles_per_mrad` for the observer's best sensor
//!   (optical sights scaled by light level; night channels and thermals are not);
//! - Johnson criteria with the NVESD target transfer probability function
//!   `P∞ = (N/N50)^E / (1 + (N/N50)^E)`, `E = 2.7 + 0.7·(N/N50)` (Vollmerhausen & Jacobs 2004);
//! - ACQUIRE time-to-detect: detection is a Poisson process with rate `P∞ / τ`
//!   scaled by vegetation transmittance and cue factors (target moving or firing, observer
//!   moving). Recognition and identification upgrade the same way with their N50 values.
//!
//! Detections become vehicle-level tracks; reporting into the side picture is `picture.rs`.

use std::collections::HashMap;

use ai::picture::ContactLevel;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::Rng;
use sim_data::{DetectionModel, SensorKind, VehicleDef};
use terrain::ground::Ground;
use terrain::los::{Sight, sight};

use crate::world::{LocalTrack, World};

/// Sensor checks every 5th tick = 4 Hz (DESIGN.md §4.1).
pub const SENSOR_EVERY: u64 = 5;
pub const SENSOR_DT: f64 = SENSOR_EVERY as f64 * crate::DT;

pub fn uniform(rng: &mut ChaCha8Rng) -> f64 {
    (rng.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
}

/// NVESD target transfer probability function.
pub fn ttpf(n: f64, n50: f64) -> f64 {
    if n <= 0.0 {
        return 0.0;
    }
    let r = n / n50;
    let e = 2.7 + 0.7 * r;
    let p = libm::pow(r, e);
    p / (1.0 + p)
}

/// Optical effectiveness by local time of day (fixed sunrise 06:00 / sunset 20:00 for now).
pub fn light_factor(clock_s: f64, m: &DetectionModel) -> f64 {
    let h = (clock_s / 3600.0).rem_euclid(24.0);
    if (6.5..19.5).contains(&h) {
        m.light_day.get()
    } else if (5.0..6.5).contains(&h) || (19.5..21.0).contains(&h) {
        m.light_twilight.get()
    } else {
        m.light_night.get()
    }
}

/// Target sample heights for partial visibility: track line, mid hull, hull roof, turret,
/// sight head.
pub fn sample_heights(def: &VehicleDef) -> [f64; 5] {
    let d = &def.dimensions;
    let (hull, top) = (d.hull_height_m.get(), d.height_m.get());
    [0.3, hull * 0.5, hull * 0.95, (hull + top) / 2.0, top * 0.97]
}

/// Projected target width seen from bearing `from_observer` (compass rad from target to observer).
pub fn projected_width(def: &VehicleDef, target_heading: f64, bearing_to_observer: f64) -> f64 {
    let a = bearing_to_observer - target_heading;
    let (w, l) = (def.dimensions.width_m.get(), def.dimensions.hull_length_m.get());
    (libm::cos(a).abs() * w + libm::sin(a).abs() * l).max(w)
}

/// Cache of sight results for pairs whose positions have not changed (≥ 0.5 m).
#[derive(Default)]
pub struct LosCache {
    map: HashMap<(u32, u32), ([i64; 4], Sight)>,
}

impl LosCache {
    #[allow(clippy::too_many_arguments)]
    pub fn get(
        &mut self,
        g: &Ground,
        key: (u32, u32),
        from: [f64; 2],
        eye: f64,
        to: [f64; 2],
        heights: &[f64],
    ) -> Sight {
        let q = [(from[0] * 2.0) as i64, (from[1] * 2.0) as i64, (to[0] * 2.0) as i64, (to[1] * 2.0) as i64];
        if let Some((k, s)) = self.map.get(&key)
            && *k == q
        {
            return s.clone();
        }
        let s = sight(g, from, eye, to, heights);
        self.map.insert(key, (q, s.clone()));
        s
    }
}

/// A new detection or upgrade that should be reported.
pub struct Sighting {
    pub observer: usize,
    pub target: usize,
    pub level: ContactLevel,
    pub new: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn step(
    world: &mut World,
    vehicles: &[VehicleDef],
    g: &Ground,
    m: &DetectionModel,
    clock_s: f64,
    tick: u64,
    rng: &mut ChaCha8Rng,
    cache: &mut LosCache,
) -> Vec<Sighting> {
    let light = light_factor(clock_s, m);
    let max_r = m.max_range_m.get();
    let timeout = (m.track_timeout_s.get() / crate::DT) as u64;
    let mut out = Vec::new();
    let n = world.len();
    for o in 0..n {
        if !world.alive(o) || world.sensors[o].is_none() {
            continue;
        }
        let Some(ovi) = world.vehicle[o] else { continue };
        let odef = &vehicles[ovi];
        let opos = world.transform[o].pos;
        let eye = odef.dimensions.sight_height_m.get();
        let observer_moving = world.mobility[o].as_ref().is_some_and(|mv| mv.speed > 1.0);
        for t in 0..n {
            if world.side[t] == world.side[o] || !world.alive(t) {
                continue;
            }
            let Some(tvi) = world.vehicle[t] else { continue };
            let tdef = &vehicles[tvi];
            let tp = world.transform[t];
            let r = crate::dist(opos, tp.pos);
            if r > max_r {
                continue;
            }
            // Distant pairs are checked at 1 Hz (staggered), with the hazard over 1 s.
            let stride: u64 = if r > 2500.0 { 4 } else { 1 };
            let phase = (world.id[o].0 as u64 * 31 + world.id[t].0 as u64) % stride;
            if !(tick / SENSOR_EVERY + phase).is_multiple_of(stride) {
                continue;
            }
            let check_dt = SENSOR_DT * stride as f64;
            let heights = sample_heights(tdef);
            let s = cache.get(g, (world.id[o].0, world.id[t].0), opos, eye, tp.pos, &heights);
            let f = s.fraction();
            if f <= 0.0 || s.transmittance < 0.02 {
                continue;
            }
            let bearing = libm::atan2(opos[0] - tp.pos[0], opos[1] - tp.pos[1]);
            let w = projected_width(tdef, tp.heading, bearing);
            let vis_h = tdef.dimensions.height_m.get() * f;
            let dc = libm::sqrt(w * vis_h);
            let sens = world.sensors[o].as_ref().expect("checked");
            let degrade = if sens.degraded { 0.5 } else { 1.0 };
            let cycles = sens
                .sensors
                .iter()
                .filter(|&&(_, _, max)| r <= max)
                .map(|&(kind, cpm, _)| {
                    let eff = match kind {
                        SensorKind::Optical => cpm * light,
                        SensorKind::NightChannel | SensorKind::Thermal => cpm,
                    };
                    dc / r * 1000.0 * eff * degrade
                })
                .fold(0.0, f64::max);
            let moving = world.mobility[t].as_ref().is_some_and(|mv| mv.speed > 1.0);
            let fired = world.last_fired[t].is_some_and(|ft| tick.saturating_sub(ft) < (3.0 / crate::DT) as u64);
            let mut cue = 1.0;
            if moving {
                cue *= m.moving_target_factor.get();
            }
            if fired {
                cue *= m.firing_target_factor.get();
            }
            if observer_moving {
                cue *= m.observer_moving_factor.get();
            }
            let tau = m.acquire_tau_s.get();
            let hazard = |n50: f64| {
                let lambda = ttpf(cycles, n50) * s.transmittance * cue / tau;
                1.0 - libm::exp(-lambda * check_dt)
            };
            let sens = world.sensors[o].as_mut().expect("checked");
            match sens.tracks.iter_mut().find(|tr| tr.target == t) {
                Some(tr) => {
                    tr.last_seen_tick = tick;
                    tr.visible_fraction = f;
                    tr.range_m = r;
                    let next = match tr.level {
                        ContactLevel::Detected => Some((ContactLevel::Recognised, m.n50_recognise.get())),
                        ContactLevel::Recognised => Some((ContactLevel::Identified, m.n50_identify.get())),
                        ContactLevel::Identified => None,
                    };
                    if let Some((lvl, n50)) = next {
                        let u = uniform(rng);
                        if u < hazard(n50) {
                            tr.level = lvl;
                            out.push(Sighting { observer: o, target: t, level: lvl, new: false });
                        }
                    }
                }
                None => {
                    let u = uniform(rng);
                    if u < hazard(m.n50_detect.get()) {
                        sens.tracks.push(LocalTrack {
                            target: t,
                            level: ContactLevel::Detected,
                            last_seen_tick: tick,
                            last_report_tick: 0,
                            contact: None,
                            visible_fraction: f,
                            range_m: r,
                        });
                        out.push(Sighting { observer: o, target: t, level: ContactLevel::Detected, new: true });
                    }
                }
            }
        }
        // Drop tracks not seen for a while, and tracks of dead targets.
        if let Some(sens) = world.sensors[o].as_mut() {
            let cond = &world.condition;
            sens.tracks.retain(|tr| {
                tick.saturating_sub(tr.last_seen_tick) <= timeout
                    && cond[tr.target] != ai::picture::Condition::Destroyed
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ttpf_is_half_at_n50_and_monotonic() {
        assert!((ttpf(4.0, 4.0) - 0.5).abs() < 1e-12);
        assert!(ttpf(1.0, 4.0) < ttpf(2.0, 4.0) && ttpf(12.0, 4.0) > 0.95);
        assert_eq!(ttpf(0.0, 1.0), 0.0);
    }
}
