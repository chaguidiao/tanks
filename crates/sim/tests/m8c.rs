//! M8c logistics: service-station resupply at a supply point (docs/plans/M8C-LOGISTICS.md).

mod common;

use common::*;
use sim::{Addressee, Command, Event, PlatoonReport, SideId, Sim};
use sim_data::SupplyPointDef;

fn setup(fuel_l: f64) -> (Sim, f64) {
    let ds = data();
    let m = map(&ds, 600, 600, "cropland", |_, _| 0.0);
    let blue: Vec<_> = (0..4)
        .map(|i| unit(&format!("1-{}", i + 1), "bulwark_a1", [600.0 + i as f64 * 60.0, 600.0], 90.0, Some("1")))
        .collect();
    let mut sc = scenario(12 * 3600, blue, vec![]);
    sc.sides[0].supply_points = vec![SupplyPointDef {
        callsign: "SP 1".into(),
        position: [1500.0, 1600.0],
        unlimited: false,
        fuel_l,
        rounds: vec![("apfsds_120a".into(), 40)],
        smoke_salvos: 0,
    }];
    let cap = ds.vehicle("bulwark_a1").unwrap().mobility.fuel_capacity_l.get();
    (Sim::new(&sc, ds, m).unwrap(), cap)
}

#[test]
fn a_platoon_drives_to_the_point_is_served_two_at_a_time_and_reports() {
    let (mut sim, cap) = setup(5000.0);
    let blue = SideId(0);
    let pic = sim.perceived(blue);
    let (pl, sp) = (pic.platoons[0].id, pic.supply_points[0].clone());
    assert_eq!((sp.callsign.as_str(), sp.fuel_l), ("SP 1", 5000.0));
    // A 3 km road march out and back burns a few hundred litres a tank.
    sim.submit(
        blue,
        Command::PlatoonMove {
            platoon: pl,
            destination: [600.0, 600.0],
            via: vec![[2200.0, 2200.0]],
            formation: None,
            technique: sim_data::Technique::Traveling,
            speed_limit_mps: None,
        },
    );
    run(&mut sim, 900.0);
    let burned: f64 = {
        let w = sim.truth();
        (0..w.len()).map(|k| cap - w.mobility[k].as_ref().unwrap().fuel_l).sum()
    };
    assert!(burned > 100.0, "the march burned fuel: {burned:.0} l");
    sim.submit(blue, Command::Resupply { platoon: pl, point: sp.id });
    let radius = 40.0;
    let (mut report, mut most_at_point) = (None, 0);
    for s in 0..1200 {
        for e in run(&mut sim, 1.0) {
            if let Event::PlatoonReport { report: PlatoonReport::Resupplied { complete }, .. } = e {
                report = Some((s, complete));
            }
        }
        let w = sim.truth();
        let at = (0..w.len())
            .filter(|&k| dist(w.transform[k].pos, sp.pos) <= radius)
            .filter(|&k| w.mobility[k].as_ref().unwrap().speed.abs() < 0.5)
            .count();
        most_at_point = most_at_point.max(at);
        if report.is_some() {
            break;
        }
    }
    let (s, complete) = report.expect("resupply finishes within 20 minutes");
    assert!(complete, "the point holds enough fuel for four tanks");
    assert!(most_at_point <= 2, "two stations: at most two vehicles served at once ({most_at_point})");
    let w = sim.truth();
    for k in 0..w.len() {
        let f = w.mobility[k].as_ref().unwrap().fuel_l;
        assert!(f > cap - 10.0, "{}: {f:.1} l of {cap} after resupply (idle burn only)", w.callsign[k]);
    }
    let left = sim.perceived(blue).supply_points[0].fuel_l;
    assert!(
        left < 5000.0 - burned && left > 5000.0 - burned - 4.0 * 60.0,
        "the point gave what was burned (plus the drive to it): {left:.1} l left, {burned:.0} l burned on the march"
    );
    assert!(sim.perceived(blue).platoons[0].task.starts_with("HALTED"), "holds after reporting");
    // Two at a time at 200 l/min: at least half the refuelling time of the whole platoon.
    assert!(s as f64 > burned / 200.0 * 60.0 / 2.0, "{s} s for {burned:.0} l");
}

#[test]
fn a_short_point_reports_partial_and_cannot_serve_the_other_side() {
    let (mut sim, _) = setup(1.0);
    let blue = SideId(0);
    let pl = sim.perceived(blue).platoons[0].id;
    // Red has no supply points; Blue's point id is not Red's.
    let mut ev = Vec::new();
    sim.submit(SideId(1), Command::Resupply { platoon: pl, point: 0 });
    ev.extend(run(&mut sim, 5.0));
    assert!(!ev.iter().any(|e| matches!(e, Event::OrderReceived { side: SideId(1), .. })));
    sim.submit(blue, Command::Resupply { platoon: pl, point: 0 });
    let mut report = None;
    for _ in 0..900 {
        for e in run(&mut sim, 1.0) {
            if let Event::PlatoonReport { report: PlatoonReport::Resupplied { complete }, .. } = e {
                report = Some(complete);
            }
            if let Event::OrderReceived { to: Addressee::Platoon(p), .. } = e {
                assert_eq!(p, pl);
            }
        }
        if report.is_some() {
            break;
        }
    }
    assert_eq!(report, Some(false), "1 l of fuel cannot fill four tanks");
    assert_eq!(sim.perceived(blue).supply_points[0].fuel_l, 0.0);
}
