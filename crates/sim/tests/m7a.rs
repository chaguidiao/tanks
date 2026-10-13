//! M7a acceptance tests: smoke screens block sensors per spectral band, crews screen themselves
//! when engaged, and the break-contact drill withdraws a platoon behind smoke.

mod common;

use common::*;
use sim::view::{PlatoonId, WeaponsStatus};
use sim::{Addressee, Command, Event, PlatoonReport, SideId, Sim, SmokeMode};

const BLUE: SideId = SideId(0);
const RED: SideId = SideId(1);
const DAY: u32 = 10 * 3600;

fn hold_all(sim: &mut Sim, units: &[sim::EntityId]) {
    for &u in units {
        let side = sim.truth().side[sim.truth().index_of(u).unwrap()];
        sim.submit_immediate(side, Command::WeaponsControl { to: Addressee::Unit(u), status: WeaponsStatus::Hold });
    }
}

fn open_map() -> (std::sync::Arc<sim_data::DataSet>, std::sync::Arc<terrain::BakedMap>) {
    let ds = data();
    let m = map(&ds, 400, 400, "cropland", |_, _| 0.0);
    (ds, m)
}

fn has_fresh_track(sim: &Sim, observer: usize, target: usize) -> bool {
    sim.truth().sensors[observer]
        .as_ref()
        .and_then(|s| s.track(target))
        .is_some_and(|t| sim.tick().saturating_sub(t.last_seen_tick) <= 10)
}

#[test]
fn smoke_blinds_an_optical_observer_and_the_track_times_out() {
    let (ds, m) = open_map();
    // Blue bulwark (k=0) faces the red kurgan (k=1) 700 m to the south.
    let sc = scenario(
        DAY,
        vec![unit("B1", "bulwark_a1", [800.0, 1000.0], 180.0, None)],
        vec![unit("R1", "kurgan_b", [800.0, 300.0], 0.0, None)],
    );
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    let ids = [sim.truth().id[0], sim.truth().id[1]];
    hold_all(&mut sim, &ids);
    run(&mut sim, 40.0);
    assert!(has_fresh_track(&sim, 1, 0), "red sees blue in open daylight");
    sim.submit_immediate(BLUE, Command::Smoke { to: Addressee::Unit(ids[0]), mode: SmokeMode::Screen });
    let ev = run(&mut sim, 3.0);
    assert!(ev.iter().any(|e| matches!(e, Event::SmokeLaunched { auto: false, .. })));
    assert!(!sim.perceived(BLUE).smoke.is_empty(), "the clouds are in the picture");
    run(&mut sim, 20.0);
    assert!(!has_fresh_track(&sim, 1, 0), "red lost blue behind the screen");
    assert!(sim.truth().sensors[1].as_ref().unwrap().track(0).is_none(), "the stale track was dropped");
}

#[test]
fn conventional_smoke_does_not_stop_a_thermal_imager() {
    let (ds, m) = open_map();
    // Blue bulwark (thermal, k=0) watches a red kurgan (k=1) that screens with ordinary grenades.
    let sc = scenario(
        DAY,
        vec![unit("B1", "bulwark_a1", [800.0, 300.0], 0.0, None)],
        vec![unit("R1", "kurgan_b", [800.0, 1000.0], 180.0, None)],
    );
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    let ids = [sim.truth().id[0], sim.truth().id[1]];
    hold_all(&mut sim, &ids);
    run(&mut sim, 40.0);
    assert!(has_fresh_track(&sim, 0, 1));
    sim.submit_immediate(RED, Command::Smoke { to: Addressee::Unit(ids[1]), mode: SmokeMode::Screen });
    run(&mut sim, 20.0);
    assert!(!sim.perceived(RED).smoke.is_empty());
    assert!(has_fresh_track(&sim, 0, 1), "thermal sees through a visual-only screen");
}

#[test]
fn engaged_crews_screen_themselves() {
    let (ds, m) = open_map();
    let sc = scenario(
        DAY,
        vec![unit("B1", "bulwark_a1", [800.0, 300.0], 0.0, None)],
        vec![unit("R1", "kurgan_b", [800.0, 1400.0], 180.0, None)],
    );
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    let ids = [sim.truth().id[0], sim.truth().id[1]];
    hold_all(&mut sim, &ids[..1]);
    let ev = run(&mut sim, 120.0);
    assert!(
        ev.iter().any(|e| matches!(e, Event::SmokeLaunched { unit, auto: true } if *unit == ids[0])),
        "the bulwark screens itself once shot at"
    );
    assert!(sim.truth().smoke[0].salvos < 2);
}

#[test]
fn break_contact_withdraws_the_platoon_behind_smoke() {
    let (ds, m) = open_map();
    let blue: Vec<_> = (0..4)
        .map(|i| unit(&format!("1-{}", i + 1), "bulwark_a1", [650.0 + i as f64 * 100.0, 700.0], 0.0, Some("1")))
        .collect();
    let sc = scenario(DAY, blue, vec![unit("R1", "kurgan_b", [800.0, 1400.0], 180.0, None)]);
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    let ids: Vec<_> = (0..5).map(|k| sim.truth().id[k]).collect();
    hold_all(&mut sim, &ids);
    run(&mut sim, 40.0);
    assert!(!sim.perceived(BLUE).contacts.is_empty(), "the platoon knows of the kurgan");
    let y0 = (0..4).map(|k| sim.truth().transform[k].pos[1]).sum::<f64>() / 4.0;
    sim.submit_immediate(BLUE, Command::BreakContact { platoon: PlatoonId(1) });
    let ev = run(&mut sim, 90.0);
    assert!(ev.iter().any(|e| matches!(e, Event::PlatoonReport { report: PlatoonReport::BreakingContact { .. }, .. })));
    assert!(ev.iter().filter(|e| matches!(e, Event::SmokeLaunched { .. })).count() >= 4);
    assert!(
        !ev.iter().any(|e| matches!(e, Event::PlatoonReport { report: PlatoonReport::Contact { .. }, .. })),
        "the drill does not halt to react to contact while withdrawing"
    );
    let y1 = (0..4).map(|k| sim.truth().transform[k].pos[1]).sum::<f64>() / 4.0;
    assert!(y0 - y1 > 300.0, "withdrew away from the threat: {y0} -> {y1}");
}
