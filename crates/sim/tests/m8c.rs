//! M8c logistics: service-station resupply at a supply point and scenario re-arm events
//! (docs/plans/M8C-LOGISTICS.md).

mod common;

use common::*;
use sim::{Addressee, Command, Event, FireReport, LogpacKind, PlatoonReport, Replay, SideId, Sim};
use sim_data::{BatteryAssign, LogisticsEventDef, LogisticsKind, ScenarioDef, SupplyPointDef};

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
    let (mut most_served, mut saw_waiting, mut saw_point) = (0, false, false);
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
        let pic = sim.perceived(blue);
        let stage = |s: sim::view::ServiceStage| pic.own.iter().filter(|u| u.service == Some(s)).count();
        most_served = most_served.max(stage(sim::view::ServiceStage::Served));
        saw_waiting |= stage(sim::view::ServiceStage::Waiting) > 0;
        saw_point |= pic.platoons[0].resupply_at == Some(sp.id);
        if report.is_some() {
            break;
        }
    }
    let (s, complete) = report.expect("resupply finishes within 20 minutes");
    assert!(complete, "the point holds enough fuel for four tanks");
    assert!(most_at_point <= 2, "two stations: at most two vehicles served at once ({most_at_point})");
    assert!(most_served == 2 && saw_waiting && saw_point, "the picture shows the stages ({most_served} served)");
    assert!(sim.perceived(blue).own.iter().all(|u| u.service.is_none() && u.ammo <= u.full_load));
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

/// Two Blue platoons ("1" and "2") of two tanks each, a battery with 10 HE, and the given re-arm
/// events.
fn logpac_scenario(events: Vec<LogisticsEventDef>) -> ScenarioDef {
    let blue = vec![
        unit("1-1", "bulwark_a1", [600.0, 600.0], 90.0, Some("1")),
        unit("1-2", "bulwark_a1", [660.0, 600.0], 90.0, Some("1")),
        unit("2-1", "bulwark_a1", [600.0, 900.0], 90.0, Some("2")),
        unit("2-2", "bulwark_a1", [660.0, 900.0], 90.0, Some("2")),
    ];
    let mut sc = scenario(12 * 3600, blue, vec![]);
    sc.sides[0].fire_support = vec![BatteryAssign {
        callsign: "ARTY 1".into(),
        battery: "sph155_btry".into(),
        position: [200.0, 200.0],
        he_rounds: Some(10),
        smoke_rounds: Some(0),
        resupplies: None,
    }];
    sc.logistics = events;
    sc
}

fn event(at_s: f64, platoons: &[&str], kind: LogisticsKind) -> LogisticsEventDef {
    LogisticsEventDef { at_s, side: 0, platoons: platoons.iter().map(|s| s.to_string()).collect(), kind }
}

/// Empty every tank's guns and smoke and half its fuel.
fn deplete(sim: &mut Sim) {
    let w = sim.truth_mut();
    for k in 0..w.len() {
        w.gun[k].as_mut().unwrap().stock.iter_mut().for_each(|s| *s = 0);
        w.smoke[k].salvos = 0;
        w.mobility[k].as_mut().unwrap().fuel_l *= 0.5;
    }
}

fn logpac_reports(ev: &[Event]) -> Vec<(sim::view::PlatoonId, bool)> {
    ev.iter()
        .filter_map(|e| match e {
            Event::PlatoonReport { platoon, report: PlatoonReport::LogpacReceived { refuel }, .. } => {
                Some((*platoon, *refuel))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_rearm_event_refills_the_listed_platoon_at_its_time() {
    let ds = data();
    let m = map(&ds, 300, 300, "cropland", |_, _| 0.0);
    let sc = logpac_scenario(vec![event(60.0, &["1"], LogisticsKind::Rearm { fraction: 0.5 })]);
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    deplete(&mut sim);
    let cap = sim.truth().gun[0].as_ref().unwrap().capacity.clone();
    let stock = |sim: &Sim, k: usize| sim.truth().gun[k].as_ref().unwrap().stock.clone();
    let before = run(&mut sim, 59.9);
    assert!(stock(&sim, 0).iter().all(|&s| s == 0), "nothing before the event time");
    assert!(logpac_reports(&before).is_empty());
    let ev = run(&mut sim, 1.0);
    let half: Vec<u32> = cap.iter().map(|&c| c / 2).collect();
    for k in 0..4 {
        let cs = sim.truth().callsign[k].clone();
        let want = if cs.starts_with('1') { half.clone() } else { vec![0; cap.len()] };
        assert_eq!(stock(&sim, k), want, "{cs}: half of every round type (rounded down), platoon 1 only");
    }
    assert_eq!((sim.truth().smoke[0].salvos, sim.truth().smoke[2].salvos), (1, 0), "half the smoke salvos");
    let p1 = sim.perceived(SideId(0)).platoons.iter().find(|p| p.callsign == "1").unwrap().id;
    assert_eq!(logpac_reports(&ev), vec![(p1, false)], "platoon 1 reports the LOGPAC");
    // The generated order is in the command log but not in the replay input.
    assert!(sim.command_log().iter().any(|c| c.generated && matches!(c.cmd, Command::Logpac { .. })));
    assert!(!sim.player_log().iter().any(|c| matches!(c.cmd, Command::Logpac { .. })));
}

#[test]
fn rearm_and_refuel_never_exceed_the_full_load() {
    let ds = data();
    let m = map(&ds, 300, 300, "cropland", |_, _| 0.0);
    let cap_l = ds.vehicle("bulwark_a1").unwrap().mobility.fuel_capacity_l.get();
    let sc = logpac_scenario(vec![
        event(10.0, &[], LogisticsKind::Refuel { fraction: 0.8 }),
        event(10.0, &[], LogisticsKind::Rearm { fraction: 0.75 }),
        event(20.0, &[], LogisticsKind::Rearm { fraction: 0.75 }),
    ]);
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    deplete(&mut sim);
    let ev = run(&mut sim, 30.0);
    let w = sim.truth();
    for k in 0..w.len() {
        let g = w.gun[k].as_ref().unwrap();
        assert_eq!(g.stock, g.capacity, "{}: two 75 % LOGPACs fill to the full load, no more", w.callsign[k]);
        let f = w.mobility[k].as_ref().unwrap().fuel_l;
        assert!(f <= cap_l && f > cap_l - 5.0, "{}: {f:.1} l of {cap_l} (capped, idle burn only)", w.callsign[k]);
    }
    let r = logpac_reports(&ev);
    let n = |refuel: bool| r.iter().filter(|x| x.1 == refuel).count();
    assert_eq!((n(true), n(false)), (2, 4), "an empty list means every platoon");
}

#[test]
fn a_battery_event_restocks_the_battery_and_players_cannot_issue_logpacs() {
    let ds = data();
    let m = map(&ds, 300, 300, "cropland", |_, _| 0.0);
    let sc =
        logpac_scenario(vec![event(30.0, &[], LogisticsKind::Battery { callsign: "ARTY 1".into(), he: 24, smoke: 6 })]);
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    let blue = SideId(0);
    let mut resupplied = Vec::new();
    for e in run(&mut sim, 60.0) {
        if let Event::FireSupport { report: FireReport::Resupplied { he, smoke }, battery, .. } = e {
            resupplied.push((battery, he, smoke));
        }
    }
    assert_eq!(resupplied, vec![(0, 24, 6)]);
    let b = &sim.perceived(blue).fire_support[0];
    assert_eq!((b.he_rounds, b.smoke_rounds), (34, 6), "10 HE at the start, 24 HE and 6 smoke arrived");

    // A LOGPAC is the scenario's: the same order given from outside is ignored.
    sim.truth_mut().gun[0].as_mut().unwrap().stock[1] = 0;
    sim.submit_immediate(blue, Command::Logpac { platoons: vec![], kind: LogpacKind::Rearm { fraction: 1.0 } });
    let ev = run(&mut sim, 5.0);
    assert_eq!(sim.truth().gun[0].as_ref().unwrap().stock[1], 0);
    assert!(logpac_reports(&ev).is_empty());
}

#[test]
fn replays_regenerate_logpacs() {
    let ds = data();
    let m = map(&ds, 300, 300, "cropland", |_, _| 0.0);
    let sc = logpac_scenario(vec![
        event(20.0, &["2"], LogisticsKind::Refuel { fraction: 1.0 }),
        event(25.0, &[], LogisticsKind::Battery { callsign: "ARTY 1".into(), he: 5, smoke: 0 }),
    ]);
    let mut sim = Sim::new(&sc, ds.clone(), m.clone()).unwrap();
    let pl = sim.perceived(SideId(0)).platoons[1].id;
    sim.submit(
        SideId(0),
        Command::PlatoonMove {
            platoon: pl,
            destination: [900.0, 900.0],
            via: vec![],
            formation: None,
            technique: sim_data::Technique::Traveling,
            speed_limit_mps: None,
        },
    );
    run(&mut sim, 40.0);
    let rec = Replay::from_ron(&Replay::record(&sim).to_ron()).unwrap();
    assert!(rec.commands.iter().all(|c| !matches!(c.cmd, Command::Logpac { .. })));
    let mut again = Sim::new(&sc, ds, m).unwrap();
    rec.play(&mut again, |_, _| {});
    assert_eq!(again.state_hash(), rec.final_hash, "replay diverged");
    assert_eq!(again.perceived(SideId(0)).fire_support[0].he_rounds, 15);
}

#[test]
fn bad_events_are_rejected_at_load() {
    let ds = data();
    let m = map(&ds, 100, 100, "cropland", |_, _| 0.0);
    for (e, why) in [
        (event(10.0, &["9"], LogisticsKind::Rearm { fraction: 1.0 }), "no platoon '9'"),
        (event(10.0, &[], LogisticsKind::Battery { callsign: "NONE".into(), he: 1, smoke: 0 }), "no battery 'NONE'"),
        (event(-1.0, &[], LogisticsKind::Refuel { fraction: 1.0 }), "bad time"),
        (event(10.0, &[], LogisticsKind::Refuel { fraction: f64::NAN }), "bad fraction"),
    ] {
        let err = Sim::new(&logpac_scenario(vec![e]), ds.clone(), m.clone()).err().expect(why);
        assert!(err.contains(why), "{err}");
    }
}
