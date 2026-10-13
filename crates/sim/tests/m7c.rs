//! M7c acceptance tests: sun and moon, time of day, fog/dust/rain, thermal vs optical.

mod common;

use common::*;
use sim::{SideId, Sim};
use sim_data::WeatherDef;

const BLUE: SideId = SideId(0);
const NOON: u32 = 12 * 3600;
const MIDNIGHT: u32 = 3600;

/// Blue `observer` at the origin of a flat map, one Red kurgan `range` metres north, stationary.
fn contact_after(observer: &str, range: f64, start: u32, weather: WeatherDef, seconds: f64) -> bool {
    let ds = data();
    let m = map(&ds, 600, 600, "cropland", |_, _| 0.0);
    let mut sc = scenario(
        start,
        vec![unit("B1", observer, [1200.0, 300.0], 0.0, None)],
        vec![unit("R1", "kurgan_b", [1200.0, 300.0 + range], 180.0, None)],
    );
    sc.weather = weather;
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    run(&mut sim, seconds);
    !sim.perceived(BLUE).contacts.is_empty()
}

fn preset(id: &str) -> WeatherDef {
    WeatherDef { preset: id.into(), ..WeatherDef::default() }
}

#[test]
fn fog_blinds_every_sensor_at_one_kilometre() {
    assert!(contact_after("bulwark_a1", 1000.0, NOON, preset("clear"), 60.0));
    assert!(!contact_after("bulwark_a1", 1000.0, NOON, preset("fog"), 60.0));
}

#[test]
fn thermal_imagers_see_through_dust_that_blinds_optics() {
    let at = 1500.0;
    assert!(contact_after("skif_m1", at, NOON, preset("clear"), 60.0), "the IFV sees by day in clear air");
    assert!(!contact_after("skif_m1", at, NOON, preset("dust"), 60.0), "dust blinds the optical IFV");
    assert!(contact_after("bulwark_a1", at, NOON, preset("dust"), 90.0), "the tank's thermal sight sees through");
}

#[test]
fn darkness_blinds_optics_but_not_the_thermal_sight() {
    assert!(contact_after("skif_m1", 1000.0, NOON, preset("clear"), 60.0));
    assert!(!contact_after("skif_m1", 1000.0, MIDNIGHT, preset("clear"), 60.0), "no moon, no light, beyond IR range");
    assert!(contact_after("bulwark_a1", 1000.0, MIDNIGHT, preset("clear"), 90.0), "thermal works at night");
}

#[test]
fn the_picture_carries_the_weather_and_sun() {
    let ds = data();
    let m = map(&ds, 100, 100, "cropland", |_, _| 0.0);
    let mut sc = scenario(MIDNIGHT, vec![unit("B1", "bulwark_a1", [100.0, 100.0], 0.0, None)], vec![]);
    sc.weather = preset("fog");
    let sim = Sim::new(&sc, ds, m).unwrap();
    let w = &sim.perceived(BLUE).weather;
    assert_eq!(w.label, "FOG");
    assert!(w.sun_elevation_deg < -10.0 && w.light < 0.1, "{w:?}");
    assert!(w.visibility_m < 500.0);
}

#[test]
fn an_unknown_preset_is_refused() {
    let ds = data();
    let m = map(&ds, 100, 100, "cropland", |_, _| 0.0);
    let mut sc = scenario(NOON, vec![unit("B1", "bulwark_a1", [100.0, 100.0], 0.0, None)], vec![]);
    sc.weather = preset("blizzard");
    assert!(Sim::new(&sc, ds, m).is_err());
}
