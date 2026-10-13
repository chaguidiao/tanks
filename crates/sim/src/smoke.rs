//! Smoke clouds (DESIGN.md §6.1, decision 18).
//!
//! A cloud is a disc that grows, drifts with the wind and fades. Line of sight is untouched:
//! the sensor model asks for the transmittance of a path in a spectral band, which is
//! `exp(-Σ τ_band · strength · chord / diameter)` over the clouds the path crosses
//! (Beer–Lambert; FM 3-11.50 for the qualitative behaviour of screens).

use ai::picture::SmokeView;
use ai::picture::{Condition, SideId};
use sim_data::{DoctrinePack, SmokeModel, VehicleDef};
use terrain::hash::StableHasher;

use crate::world::World;
use crate::{Event, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Band {
    /// Visual and near-IR: direct optics and image intensifiers.
    Visible,
    /// Thermal imagers, 8–12 µm.
    Thermal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cloud {
    pub kind: usize,
    pub pos: Vec2,
    pub age_s: f64,
}

#[derive(Debug, Clone)]
pub struct SmokeField {
    model: SmokeModel,
    pub blind_below: f64,
    pub clouds: Vec<Cloud>,
    /// Wind velocity (m/s east, north): where the air goes.
    wind: Vec2,
}

impl SmokeField {
    pub fn new(model: &SmokeModel, wind_from_deg: f64, wind_speed_mps: f64) -> Self {
        let to = (wind_from_deg + 180.0).to_radians();
        Self {
            model: model.clone(),
            blind_below: model.blind_below.get(),
            clouds: Vec::new(),
            wind: [libm::sin(to) * wind_speed_mps, libm::cos(to) * wind_speed_mps],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.clouds.is_empty()
    }

    pub fn add(&mut self, kind: usize, pos: Vec2) {
        self.clouds.push(Cloud { kind, pos, age_s: 0.0 });
    }

    /// Advance by `dt` seconds: age, drift, drop the expired.
    pub fn step(&mut self, dt: f64) {
        for c in &mut self.clouds {
            let k = &self.model.kinds[c.kind];
            c.age_s += dt;
            c.pos[0] += self.wind[0] * k.drift.get() * dt;
            c.pos[1] += self.wind[1] * k.drift.get() * dt;
        }
        let kinds = &self.model.kinds;
        self.clouds.retain(|c| c.age_s < kinds[c.kind].life_s.get());
    }

    /// Current radius (m).
    pub fn radius(&self, c: &Cloud) -> f64 {
        let k = &self.model.kinds[c.kind];
        k.radius_m.get() * ramp(c.age_s, k.rise_s.get())
    }

    /// Current strength, 0..1: grows with the cloud, fades over the last half of its life.
    pub fn strength(&self, c: &Cloud) -> f64 {
        let k = &self.model.kinds[c.kind];
        let life = k.life_s.get();
        ramp(c.age_s, k.rise_s.get()) * ((life - c.age_s) / (0.5 * life)).clamp(0.0, 1.0)
    }

    /// Transmittance (0..1) of the straight path `a` → `b` in `band`.
    pub fn transmittance(&self, a: Vec2, b: Vec2, band: Band) -> f64 {
        let mut tau = 0.0;
        for c in &self.clouds {
            let r = self.radius(c);
            let chord = chord_length(a, b, c.pos, r);
            if chord > 0.0 {
                let k = &self.model.kinds[c.kind];
                let depth = match band {
                    Band::Visible => k.tau_visible.get(),
                    Band::Thermal => k.tau_thermal.get(),
                };
                tau += depth * self.strength(c) * chord / (2.0 * k.radius_m.get());
            }
        }
        libm::exp(-tau)
    }

    pub fn views(&self) -> Vec<SmokeView> {
        self.clouds
            .iter()
            .map(|c| SmokeView { pos: c.pos, radius: self.radius(c), strength: self.strength(c) })
            .collect()
    }

    pub(crate) fn hash_into(&self, h: &mut StableHasher) {
        h.u64(self.clouds.len() as u64);
        for c in &self.clouds {
            h.u64(c.kind as u64);
            h.f64(c.pos[0]);
            h.f64(c.pos[1]);
            h.f64(c.age_s);
        }
    }
}

impl SmokeField {
    /// Kind of cloud for a launcher or engine system in `year`.
    pub fn pick(&self, ids: &[String], year: i32) -> Option<usize> {
        self.model.pick(ids, year)
    }
}

/// A salvo is fired this long after a round last struck near the vehicle, at the latest (s).
const REACT_WINDOW_S: f64 = 2.0;
/// Minimum time between salvos from one vehicle (s): crews do not empty the launcher at once.
const SALVO_INTERVAL_S: f64 = 20.0;
/// Engine smoke puffs, one per vehicle every this many ticks.
const ENGINE_PUFF_EVERY: u64 = 20;

/// Fire one grenade salvo from vehicle `k` towards compass bearing `toward`.
pub fn launch(
    field: &mut SmokeField,
    world: &mut World,
    vehicles: &[VehicleDef],
    k: usize,
    toward: f64,
    year: i32,
    tick: u64,
) -> bool {
    let Some(vi) = world.vehicle[k] else { return false };
    let Some(l) = vehicles[vi].smoke_launcher.as_ref() else { return false };
    if world.smoke[k].salvos == 0 || !world.alive(k) {
        return false;
    }
    let Some(kind) = field.pick(&l.kinds, year) else { return false };
    world.smoke[k].salvos -= 1;
    world.smoke[k].last_launch = Some(tick);
    let pos = world.transform[k].pos;
    let n = l.clouds_per_salvo.max(1);
    let arc = l.arc_deg.get().to_radians();
    for i in 0..n {
        let off = if n == 1 { 0.0 } else { (i as f64 / (n - 1) as f64 - 0.5) * arc };
        let b = toward + off;
        field.add(kind, [pos[0] + libm::sin(b) * l.range_m.get(), pos[1] + libm::cos(b) * l.range_m.get()]);
    }
    true
}

/// Per-tick smoke handling for vehicles: self-screening when engaged (ATP 3-20.15 App. F, react
/// to contact: employ smoke) and engine smoke puffs behind moving vehicles.
pub fn units(
    field: &mut SmokeField,
    world: &mut World,
    vehicles: &[VehicleDef],
    doctrines: &[&DoctrinePack],
    years: &[i32],
    tick: u64,
    events: &mut Vec<Event>,
) {
    let react = (REACT_WINDOW_S / crate::DT) as u64;
    let interval = (SALVO_INTERVAL_S / crate::DT) as u64;
    for k in 0..world.len() {
        if world.condition[k] == Condition::Destroyed {
            continue;
        }
        let side: SideId = world.side[k];
        let year = years[side.0 as usize];
        if tick.is_multiple_of(crate::VEHICLE_AI_EVERY)
            && doctrines[side.0 as usize].smoke_when_engaged
            && let Some((t, bearing)) = world.smoke[k].last_targeted
            && tick.saturating_sub(t) <= react
            && world.smoke[k].last_launch.is_none_or(|l| tick.saturating_sub(l) >= interval)
            && launch(field, world, vehicles, k, bearing, year, tick)
        {
            events.push(Event::SmokeLaunched { unit: world.id[k], auto: true });
        }
        if world.smoke[k].engine_on
            && (tick + world.id[k].0 as u64).is_multiple_of(ENGINE_PUFF_EVERY)
            && world.mobility[k].as_ref().is_some_and(|m| m.speed > 1.0)
            && let Some(vi) = world.vehicle[k]
            && let Some(e) = vehicles[vi].engine_smoke.as_ref()
            && let Some(kind) = field.pick(&e.kinds, year)
        {
            let t = &world.transform[k];
            let back = vehicles[vi].dimensions.hull_length_m.get() * 0.5;
            field.add(kind, [t.pos[0] - libm::sin(t.heading) * back, t.pos[1] - libm::cos(t.heading) * back]);
        }
    }
}

/// 0.3 → 1 over `rise` seconds: a puff that blooms.
fn ramp(age: f64, rise: f64) -> f64 {
    0.3 + 0.7 * (age / rise).clamp(0.0, 1.0)
}

/// Length of segment `a`→`b` inside the disc (`c`, `r`).
pub fn chord_length(a: Vec2, b: Vec2, c: Vec2, r: f64) -> f64 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let len2 = d[0] * d[0] + d[1] * d[1];
    if len2 < 1e-12 {
        return 0.0;
    }
    let f = [a[0] - c[0], a[1] - c[1]];
    let (qa, qb, qc) = (len2, 2.0 * (f[0] * d[0] + f[1] * d[1]), f[0] * f[0] + f[1] * f[1] - r * r);
    let disc = qb * qb - 4.0 * qa * qc;
    if disc <= 0.0 {
        return 0.0;
    }
    let s = libm::sqrt(disc);
    let (t0, t1) = (((-qb - s) / (2.0 * qa)).clamp(0.0, 1.0), ((-qb + s) / (2.0 * qa)).clamp(0.0, 1.0));
    (t1 - t0) * libm::sqrt(len2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> SmokeModel {
        let text =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/combat/smoke.ron")).unwrap();
        sim_data::ron_options().from_str(&text).unwrap()
    }

    #[test]
    fn chord_through_centre_is_the_diameter() {
        assert!((chord_length([-50.0, 0.0], [50.0, 0.0], [0.0, 0.0], 10.0) - 20.0).abs() < 1e-9);
        // Offset path: chord = 2·sqrt(r² − d²) = 2·sqrt(100 − 36) = 16.
        assert!((chord_length([-50.0, 6.0], [50.0, 6.0], [0.0, 0.0], 10.0) - 16.0).abs() < 1e-9);
        assert_eq!(chord_length([-50.0, 11.0], [50.0, 11.0], [0.0, 0.0], 10.0), 0.0);
        // A path that ends inside the disc counts only the part inside.
        assert!((chord_length([-50.0, 0.0], [0.0, 0.0], [0.0, 0.0], 10.0) - 10.0).abs() < 1e-9);
        assert_eq!(chord_length([20.0, 0.0], [50.0, 0.0], [0.0, 0.0], 10.0), 0.0);
    }

    #[test]
    fn grown_screen_blocks_visible_but_a_conventional_one_passes_thermal() {
        let m = model();
        let mut f = SmokeField::new(&m, 0.0, 0.0);
        f.add(m.kind("grenade").unwrap(), [0.0, 0.0]);
        f.step(5.0);
        let (a, b) = ([-200.0, 0.0], [200.0, 0.0]);
        let vis = f.transmittance(a, b, Band::Visible);
        let th = f.transmittance(a, b, Band::Thermal);
        assert!(vis < f.blind_below, "visible {vis}");
        assert!(th > 0.3, "thermal {th}");
        // A path that misses the cloud is untouched.
        assert!((f.transmittance([-200.0, 100.0], [200.0, 100.0], Band::Visible) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn multispectral_screen_blocks_the_thermal_band() {
        let m = model();
        let mut f = SmokeField::new(&m, 0.0, 0.0);
        f.add(m.kind("grenade_ms").unwrap(), [0.0, 0.0]);
        f.step(5.0);
        assert!(f.transmittance([-200.0, 0.0], [200.0, 0.0], Band::Thermal) < f.blind_below);
    }

    #[test]
    fn clouds_drift_downwind_and_expire() {
        let m = model();
        // Wind from the west at 4 m/s carries the cloud east.
        let mut f = SmokeField::new(&m, 270.0, 4.0);
        f.add(m.kind("grenade").unwrap(), [0.0, 0.0]);
        for _ in 0..(10 * crate::TICK_HZ) {
            f.step(crate::DT);
        }
        let c = &f.clouds[0];
        assert!((c.pos[0] - 40.0).abs() < 0.5 && c.pos[1].abs() < 0.5, "{:?}", c.pos);
        for _ in 0..(60 * crate::TICK_HZ) {
            f.step(crate::DT);
        }
        assert!(f.is_empty());
    }

    #[test]
    fn strength_fades_over_the_second_half_of_life() {
        let m = model();
        let mut f = SmokeField::new(&m, 0.0, 0.0);
        f.add(m.kind("grenade").unwrap(), [0.0, 0.0]);
        f.step(10.0);
        assert!((f.strength(&f.clouds[0]) - 1.0).abs() < 1e-9);
        f.step(30.0);
        let s = f.strength(&f.clouds[0]);
        assert!(s > 0.0 && s < 1.0, "{s}");
    }
}
