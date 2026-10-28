//! Sensors and detection (DESIGN.md §6.1), run at 4 Hz.
//!
//! For each observer/target pair with line of sight:
//! - critical dimension `d_c = sqrt(projected width × visible height)` from the target aspect
//!   and the LOS visible fraction;
//! - resolvable cycles `N = d_c / R · 1000 · cycles_per_mrad` for the observer's best sensor
//!   (optical sights scaled by light level, thermals by thermal contrast; atmospheric
//!   transmittance and smoke per band);
//! - Johnson criteria with the NVESD target transfer probability function
//!   `P∞ = (N/N50)^E / (1 + (N/N50)^E)`, `E = 2.7 + 0.7·(N/N50)` (Vollmerhausen & Jacobs 2004);
//! - ACQUIRE time-to-detect: detection is a Poisson process with rate `P∞ / τ`
//!   scaled by vegetation transmittance and cue factors (target moving or firing, observer
//!   moving). Recognition and identification upgrade the same way with their N50 values.
//! - Field-of-regard search (B24 D2): a first detection through a sight whose field of view is
//!   narrower than the search sector runs at `fov / sector` of its in-view rate; the crew's best
//!   sight counts, so the commander's wide view finds big, near targets and the magnified sights
//!   far, small ones slowly. A target that fires shows where to look (no search penalty).
//!
//! Detections become vehicle-level tracks; reporting into the side picture is `picture.rs`.

use std::collections::HashMap;

use ai::picture::ContactLevel;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::Rng;
use sim_data::{DetectionModel, InfantryModel, SensorKind, SquadDef, TerrainData, VehicleDef};
use terrain::ground::Ground;
use terrain::los::{Sight, sight};

use crate::smoke::Band;
use crate::weather::Atmosphere;
use crate::world::{LocalTrack, Posture, World};

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

/// A launch cloud and the firing cue last this many ticks (3 s).
const LAUNCH_SEEN: u64 = (3.0 / crate::DT) as u64;

/// Speed above which a vehicle raises a visible dust plume (m/s).
const DUST_SPEED_MPS: f64 = 4.0;

fn tmoving_fast(world: &World, t: usize) -> bool {
    world.mobility[t].as_ref().is_some_and(|mv| mv.speed > DUST_SPEED_MPS)
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

/// Infantry inputs to the detection step (plan M8B-INFANTRY §3).
pub struct InfantryLook<'a> {
    pub squads: &'a [SquadDef],
    pub model: &'a InfantryModel,
    pub td: &'a TerrainData,
    /// Scenario month (tall crops are seasonal).
    pub month: u8,
}

/// Squad sample heights for partial visibility: low, middle and top of the posture's silhouette
/// (the first three entries are used).
fn squad_heights(h: f64, floor: f64) -> ([f64; 5], usize) {
    ([floor + 0.15 * h, floor + 0.5 * h, floor + 0.9 * h, 0.0, 0.0], 3)
}

#[allow(clippy::too_many_arguments)]
pub fn step(
    world: &mut World,
    vehicles: &[VehicleDef],
    inf: &InfantryLook,
    g: &Ground,
    m: &DetectionModel,
    atm: &Atmosphere,
    sensor_penalty: f64,
    tick: u64,
    rng: &mut ChaCha8Rng,
    cache: &mut LosCache,
    smoke: &crate::smoke::SmokeField,
) -> Vec<Sighting> {
    let light = atm.light;
    let max_r = m.max_range_m.get();
    let sector = m.search_sector_deg.get();
    let timeout = (m.track_timeout_s.get() / crate::DT) as u64;
    let mut out = Vec::new();
    let n = world.len();
    for o in 0..n {
        if !world.alive(o) || world.sensors[o].is_none() || world.mounted(o) {
            continue;
        }
        // Observer: a vehicle's sight, or a squad's eyes at its posture (and floor).
        let (eye, obuilding) = match (world.vehicle[o], world.squad[o].as_ref()) {
            (Some(ovi), _) => (vehicles[ovi].dimensions.sight_height_m.get(), None),
            (None, Some(s)) => {
                let (sig, floor) = crate::infantry::signature(&inf.squads[s.def], s, g.map, inf.td);
                (sig.eye_height_m.get() + floor, s.in_building)
            }
            _ => continue,
        };
        let opos0 = world.transform[o].pos;
        let observer_moving = world.mobility[o].as_ref().is_some_and(|mv| mv.speed > 1.0)
            || world.squad[o].as_ref().is_some_and(|s| s.speed > 0.3);
        for t in 0..n {
            if world.side[t] == world.side[o] || !world.alive(t) || world.mounted(t) {
                continue;
            }
            let tp = world.transform[t];
            let r = crate::dist(opos0, tp.pos);
            if r > max_r {
                continue;
            }
            // Target: a vehicle's silhouette, or a squad's posture row (window-sized in a building,
            // seen only through the facade facing the observer; low in standing crops).
            enum Look<'a> {
                Vehicle(&'a VehicleDef),
                Squad { w: f64, h: f64, floor: f64, building: Option<u32>, crops: f64, camo: f64, launch: bool },
            }
            let look = match (world.vehicle[t], world.squad[t].as_ref()) {
                (Some(tvi), _) => Look::Vehicle(&vehicles[tvi]),
                (None, Some(s)) => {
                    let (sig, floor) = crate::infantry::signature(&inf.squads[s.def], s, g.map, inf.td);
                    let (ci, cj) = g.map.elevation.cell_of(tp.pos[0], tp.pos[1]);
                    let in_crop = s.speed <= 0.3
                        && s.in_building.is_none()
                        && inf.model.tall_crops.months.contains(&inf.month)
                        && inf.td.landcover(g.map.landcover.get(ci, cj)).name == "cropland";
                    let crops = if in_crop { inf.model.tall_crops.visible_fraction.get() } else { 1.0 };
                    let c = &inf.model.camouflage;
                    let camo = match s.posture {
                        _ if s.in_building.is_some() => inf.model.camouflage_in_building.get(),
                        Posture::Moving => c.moving.get(),
                        Posture::Halted => c.halted.get(),
                        Posture::DugIn => c.dug_in.get(),
                    };
                    // A launch cloud (plan M8B-AT-TEAMS §4) rises above the men and is larger than
                    // them for a few seconds; it is not camouflaged and not hidden by crops.
                    let launch = world.launch[t].is_some_and(|(lt, _)| tick.saturating_sub(lt) < LAUNCH_SEEN);
                    let cloud = if launch { m.launch_cloud_m.get() } else { 0.0 };
                    Look::Squad {
                        w: sig.width_m.get().max(cloud),
                        h: sig.height_m.get().max(cloud),
                        floor,
                        building: s.in_building,
                        crops: if launch { 1.0 } else { crops },
                        camo: if launch { 1.0 } else { camo },
                        launch,
                    }
                }
                _ => continue,
            };
            // Distant pairs are checked at 1 Hz (staggered), with the hazard over 1 s.
            let stride: u64 = if r > 2500.0 { 4 } else { 1 };
            let phase = (world.id[o].0 as u64 * 31 + world.id[t].0 as u64) % stride;
            if !(tick / SENSOR_EVERY + phase).is_multiple_of(stride) {
                continue;
            }
            let check_dt = SENSOR_DT * stride as f64;
            let opos = match obuilding {
                Some(id) => crate::infantry::facade_point(g.map, id, opos0, tp.pos),
                None => opos0,
            };
            let (tpos, (heights, nh)) = match &look {
                Look::Vehicle(tdef) => (tp.pos, (sample_heights(tdef), 5)),
                Look::Squad { h, floor, building, .. } => (
                    building.map_or(tp.pos, |id| crate::infantry::facade_point(g.map, id, tp.pos, opos0)),
                    squad_heights(*h, *floor),
                ),
            };
            // The cache keys on positions only, so a launch cloud's taller samples bypass it.
            let s = match &look {
                Look::Squad { launch: true, .. } => sight(g, opos, eye, tpos, &heights[..nh]),
                _ => cache.get(g, (world.id[o].0, world.id[t].0), opos, eye, tpos, &heights[..nh]),
            };
            let mut f = s.fraction();
            if let Look::Squad { crops, .. } = &look {
                f *= crops;
            }
            if f <= 0.0 || s.transmittance < 0.02 {
                continue;
            }
            let (dc, thermal, optical) = match &look {
                Look::Vehicle(tdef) => {
                    let bearing = libm::atan2(opos[0] - tp.pos[0], opos[1] - tp.pos[1]);
                    let w = projected_width(tdef, tp.heading, bearing);
                    let vis_h = tdef.dimensions.height_m.get() * f;
                    (libm::sqrt(w * vis_h), atm.thermal, 1.0)
                }
                // People stay warmer than their background through the crossover; a launch is hot
                // and stands out in daylight.
                Look::Squad { w, h, camo, launch, .. } => (
                    libm::sqrt(w * h * f),
                    if *launch { 1.0 } else { 1.0 - (1.0 - atm.thermal) * inf.model.thermal_crossover_share.get() },
                    if *launch { 1.0 } else { inf.model.optical_contrast.get() * camo },
                ),
            };
            let sens = world.sensors[o].as_ref().expect("checked");
            let degrade = if sens.degraded { 0.5 } else { 1.0 } * (1.0 - sensor_penalty * world.suppression[o]);
            // Smoke on the path: per spectral band, a sensor below the blind threshold sees nothing.
            let (mut t_vis, mut t_th) = (libm::exp(-atm.beta_visible * r), libm::exp(-atm.beta_thermal * r));
            if !smoke.is_empty() {
                t_vis *= smoke.transmittance(opos, tp.pos, Band::Visible);
                t_th *= smoke.transmittance(opos, tp.pos, Band::Thermal);
            }
            let (mut in_range, mut blinded) = (0, 0);
            let looks: Vec<(f64, SensorKind, f64)> = sens
                .sensors
                .iter()
                .filter(|&&(_, _, max, _)| r <= max)
                .filter_map(|&(kind, cpm, _, fov)| {
                    let (eff, band_t) = match kind {
                        SensorKind::Optical => (cpm * light * optical, t_vis),
                        SensorKind::NightChannel => (cpm * optical, t_vis),
                        SensorKind::Thermal => (cpm * thermal, t_th),
                    };
                    in_range += 1;
                    if band_t < smoke.blind_below {
                        blinded += 1;
                        return None;
                    }
                    Some((dc / r * 1000.0 * eff * band_t * degrade, kind, (fov / sector).min(1.0)))
                })
                .collect();
            // Every sensor that could reach is screened: the pair is not seen, so tracks go stale.
            if in_range > 0 && blinded == in_range {
                continue;
            }
            let (cycles, best) =
                looks.iter().fold((0.0, SensorKind::Optical), |a, &(n, k, _)| if n > a.0 { (n, k) } else { a });
            let moving = world.mobility[t].as_ref().is_some_and(|mv| mv.speed > 1.0)
                || world.squad[t].as_ref().is_some_and(|s| s.speed > 0.3);
            let recent = |ft: u64| tick.saturating_sub(ft) < LAUNCH_SEEN;
            let fired = world.last_fired[t].is_some_and(recent);
            // Backblast of a rocket or missile launch: a stronger cue (plan M8B-AT-TEAMS §2.2).
            let launch = world.launch[t].filter(|&(lt, _)| recent(lt)).map_or(1.0, |(_, f)| f);
            let mut cue = 1.0;
            if moving {
                cue *= m.moving_target_factor.get();
                // A dust plume behind a fast vehicle is seen by eye, not by a thermal imager.
                if best != SensorKind::Thermal && tmoving_fast(world, t) {
                    cue *= atm.dust_cue;
                }
            }
            if fired {
                cue *= m.firing_target_factor.get() * launch;
            }
            if observer_moving {
                cue *= m.observer_moving_factor.get();
            }
            let tau = m.acquire_tau_s.get();
            let hazard = |p: f64| 1.0 - libm::exp(-p * s.transmittance * cue / tau * check_dt);
            // First detection: each sight finds a target outside its view only as fast as it
            // sweeps the search sector (NVESD field-of-regard search, B24 D2); the crew's best
            // look counts. A target that fires shows where to look. Upgrades are made in view.
            let search = |n50: f64| {
                looks.iter().map(|&(n, _, s)| ttpf(n, n50) * if fired { 1.0 } else { s }).fold(0.0, f64::max)
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
                        if u < hazard(ttpf(cycles, n50)) {
                            tr.level = lvl;
                            out.push(Sighting { observer: o, target: t, level: lvl, new: false });
                        }
                    }
                }
                None => {
                    let u = uniform(rng);
                    if u < hazard(search(m.n50_detect.get())) {
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
