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

// --- Slice 6: supply trucks (docs/plans/M8C-TAILGATE-TRUCKS.md) ---

/// A Blue tank platoon with a cargo and a fuel truck behind it, and the Red units given.
fn trucks_scenario(red: Vec<sim_data::UnitDef>) -> ScenarioDef {
    let mut blue: Vec<_> = (0..4)
        .map(|i| unit(&format!("1-{}", i + 1), "bulwark_a1", [600.0 + i as f64 * 60.0, 600.0], 90.0, Some("1")))
        .collect();
    blue.push(unit("SUP-1", "carter_c", [400.0, 300.0], 90.0, Some("SUP")));
    blue.push(unit("SUP-2", "carter_f", [430.0, 300.0], 90.0, Some("SUP")));
    scenario(12 * 3600, blue, red)
}

/// Plan §3: a truck without listed cargo carries its type's capacity in the rounds its side's
/// vehicles carry; a fuel truck carries fuel only.
#[test]
fn supply_trucks_carry_their_sides_rounds_by_default() {
    let ds = data();
    let m = map(&ds, 600, 600, "cropland", |_, _| 0.0);
    let sim = Sim::new(&trucks_scenario(vec![]), ds.clone(), m).unwrap();
    let w = sim.truth();
    let cap = ds.vehicle("carter_c").unwrap().cargo.clone().unwrap();
    let tank = w.gun[0].as_ref().unwrap();
    let ammo = sim.ammo_table();
    let loaded = w.cargo[4].as_ref().expect("the cargo truck has cargo");
    let main_gun: u32 = (0..ammo.len())
        .filter(|&i| ammo[i].stow_class() == sim_data::StowClass::MainGun)
        .map(|i| loaded.rounds[i])
        .sum();
    assert_eq!(main_gun, cap.main_gun_rounds);
    // Every round loaded is one the tanks carry.
    let carried: Vec<usize> =
        sim.loadout(0).iter().zip(&tank.stock).filter(|&(_, &n)| n > 0).map(|(&a, _)| a).collect();
    assert!((0..ammo.len()).all(|i| loaded.rounds[i] == 0 || carried.contains(&i)), "{:?}", loaded.rounds);
    assert_eq!(loaded.fuel_l, 0.0);
    let fuel = w.cargo[5].as_ref().expect("the fuel truck has cargo");
    assert_eq!(fuel.fuel_l, ds.vehicle("carter_f").unwrap().cargo.as_ref().unwrap().fuel_l.get());
    assert!(fuel.rounds.iter().all(|&n| n == 0));
}

#[test]
fn scenario_cargo_replaces_the_default_and_trucks_keep_to_their_own_platoon() {
    let ds = data();
    let m = map(&ds, 600, 600, "cropland", |_, _| 0.0);
    let mut sc = trucks_scenario(vec![]);
    sc.sides[0].units[4].cargo =
        Some(sim_data::CargoAssign { fuel_l: 0.0, rounds: vec![("apfsds_120a".into(), 12)], smoke_salvos: 2 });
    let sim = Sim::new(&sc, ds.clone(), m.clone()).unwrap();
    let c = sim.truth().cargo[4].clone().unwrap();
    assert_eq!((c.rounds.iter().sum::<u32>(), c.smoke_salvos), (12, 2));

    let mut mixed = trucks_scenario(vec![]);
    mixed.sides[0].units[4].platoon = Some("1".into());
    assert!(Sim::new(&mixed, ds.clone(), m.clone()).err().expect("refused").contains("platoon of their own"));
    let mut tank_cargo = trucks_scenario(vec![]);
    tank_cargo.sides[0].units[0].cargo = Some(Default::default());
    assert!(Sim::new(&tank_cargo, ds, m).err().expect("refused").contains("only a supply truck"));
}

/// Plan §6 step 2: a truck never fires; the enemy engages and destroys it; trucks are not part
/// of the force for victory (§8 #3).
#[test]
fn trucks_never_fire_are_killed_and_do_not_count_for_victory() {
    let ds = data();
    let m = map(&ds, 1200, 1200, "cropland", |_, _| 0.0);
    // Only the trucks face a Red tank 1.2 km away; the Blue tanks are 5 km off, beyond its reach.
    let mut sc = trucks_scenario(vec![unit("R1", "kurgan_b", [4200.0, 4200.0], 270.0, Some("R"))]);
    for (i, u) in sc.sides[0].units.iter_mut().take(4).enumerate() {
        u.position = [300.0 + i as f64 * 40.0, 300.0];
    }
    sc.sides[0].units[4].position = [3000.0, 4200.0];
    sc.sides[0].units[5].position = [3000.0, 4260.0];
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    let mut ended = None;
    for e in run(&mut sim, 300.0) {
        if let Event::ScenarioEnd { reason, .. } = e {
            ended = Some(reason);
        }
    }
    let w = sim.truth();
    assert!(w.last_fired[4].is_none() && w.last_fired[5].is_none(), "trucks have nothing to fire");
    assert!(w.last_fired[6].is_some(), "the Red tank fired");
    assert!(!w.alive(4) || !w.alive(5), "the Red tank destroyed a truck");
    assert!(ended.is_none(), "losing trucks does not destroy Blue's force: {ended:?}");
}

/// Plan §5: trucks are the lowest-priority target: a gunner takes a tank before a nearer truck.
#[test]
fn a_gunner_takes_a_tank_before_a_nearer_truck() {
    let ds = data();
    let m = map(&ds, 600, 600, "cropland", |_, _| 0.0);
    let mut sc = trucks_scenario(vec![unit("R1", "kurgan_b", [2200.0, 1200.0], 270.0, Some("R"))]);
    // Tank 1-1 at 1.4 km, the cargo truck at 0.8 km; the other Blue vehicles far behind.
    for (i, u) in sc.sides[0].units.iter_mut().enumerate() {
        u.position = [100.0 + i as f64 * 30.0, 100.0];
    }
    sc.sides[0].units[0].position = [800.0, 1200.0];
    sc.sides[0].units[4].position = [1400.0, 1200.0];
    for u in sc.sides[0].units.iter_mut() {
        u.heading_deg = 270.0;
    }
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    // Both sides hold fire for a minute so Red has seen everything, then Red is released.
    for side in [SideId(0), SideId(1)] {
        let pl = sim.perceived(side).platoons[0].id;
        sim.submit_immediate(
            side,
            Command::WeaponsControl { to: Addressee::Platoon(pl), status: sim::view::WeaponsStatus::Hold },
        );
    }
    run(&mut sim, 60.0);
    let seen = |sim: &Sim, k: usize| sim.truth().sensors[6].as_ref().unwrap().track(k).is_some();
    assert!(seen(&sim, 0) && seen(&sim, 4), "Red sees the tank and the truck");
    let red = sim.perceived(SideId(1)).platoons[0].id;
    sim.submit_immediate(
        SideId(1),
        Command::WeaponsControl { to: Addressee::Platoon(red), status: sim::view::WeaponsStatus::Free },
    );
    let mut first = None;
    for _ in 0..(60 * sim::TICK_HZ) {
        sim.step();
        if let Some(t) = sim.truth().gun[6].as_ref().and_then(|g| g.target) {
            first = Some(t);
            break;
        }
    }
    let first = first.expect("Red engaged");
    assert!(first < 4, "the Red tank's first target is a Blue tank, not the nearer truck: {first}");
}

/// The trucks scenario with the tanks short of rounds and fuel: each tank has fired 10 rounds of
/// its main stowage entry and burned 300 l. Returns the sim and the platoon ids (tanks, trucks).
fn depleted_platoon() -> (Sim, sim::view::PlatoonId, sim::view::PlatoonId, ScenarioDef) {
    let ds = data();
    let m = map(&ds, 600, 600, "cropland", |_, _| 0.0);
    let sc = trucks_scenario(vec![]);
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    deplete_tanks(&mut sim);
    let pic = sim.perceived(SideId(0));
    let id = |cs: &str| pic.platoons.iter().find(|p| p.callsign == cs).unwrap().id;
    let (tanks, trucks) = (id("1"), id("SUP"));
    (sim, tanks, trucks, sc)
}

fn deplete_tanks(sim: &mut Sim) {
    let w = sim.truth_mut();
    for k in 0..4 {
        let g = w.gun[k].as_mut().unwrap();
        let i = (0..g.stock.len()).max_by_key(|&i| g.stock[i]).unwrap();
        g.stock[i] -= 10;
        w.mobility[k].as_mut().unwrap().fuel_l -= 300.0;
    }
}

/// Rounds and fuel held by the tanks (0-3) and the trucks (4-5).
fn totals(sim: &Sim) -> (u32, f64, u32, f64) {
    let w = sim.truth();
    let tank_rounds = (0..4).map(|k| w.gun[k].as_ref().unwrap().rounds()).sum();
    let tank_fuel = (0..4).map(|k| w.mobility[k].as_ref().unwrap().fuel_l).sum();
    let truck_rounds = (4..6).map(|k| w.cargo[k].as_ref().unwrap().rounds.iter().sum::<u32>()).sum();
    let truck_fuel = (4..6).map(|k| w.cargo[k].as_ref().unwrap().fuel_l).sum();
    (tank_rounds, tank_fuel, truck_rounds, truck_fuel)
}

/// Plan §3 and §6 step 3: the trucks drive up, serve every tank where it stands, conserve stock,
/// stop the main gun firing while its rounds go in, report, and drive back home.
#[test]
fn tailgate_trucks_serve_the_platoon_in_place_and_drive_home() {
    let (mut sim, tanks, trucks, _) = depleted_platoon();
    let start: Vec<[f64; 2]> = (0..6).map(|k| sim.truth().transform[k].pos).collect();
    let before = totals(&sim);
    sim.submit(SideId(0), Command::Tailgate { platoon: tanks, trucks });
    let (mut done, mut returning, mut served_seen, mut loading_seen) = (None, 0, false, false);
    for s in 0..2400 {
        for e in run(&mut sim, 1.0) {
            match e {
                Event::PlatoonReport { report: PlatoonReport::Resupplied { complete }, platoon, .. } => {
                    assert_eq!(platoon, tanks);
                    done = Some((s, complete));
                }
                Event::PlatoonReport { report: PlatoonReport::TrucksReturning { .. }, platoon, .. } => {
                    assert_eq!(platoon, trucks);
                    returning += 1;
                }
                _ => {}
            }
        }
        let pic = sim.perceived(SideId(0));
        served_seen |= pic.own.iter().any(|u| u.service == Some(sim::view::ServiceStage::Served));
        loading_seen |= (0..4).any(|k| sim.truth().gun[k].as_ref().unwrap().loading_until > sim.tick());
        if done.is_some() && sim.perceived(SideId(0)).platoons.iter().find(|p| p.id == trucks).unwrap().task == "HALTED"
        {
            break;
        }
    }
    let (s, complete) = done.expect("the platoon reported");
    assert!(complete, "every tank full");
    assert_eq!(returning, 1);
    assert!(served_seen && loading_seen, "stages shown, main gun held while loading");
    let after = totals(&sim);
    assert_eq!(after.0 - before.0, 40, "40 rounds handed over");
    assert_eq!(before.2 - after.2, 40, "...and the trucks have 40 fewer");
    // Fuel: each tank is full to within a few litres (it idles while it waits; a truck does not
    // stop for less than 10 l), and the fuel truck gave about the 1200 l the tanks lacked.
    let given = before.3 - after.3;
    assert!((1150.0..1300.0).contains(&given), "fuel truck gave {given:.0} l");
    let w = sim.truth();
    let cap =
        sim_data::DataSet::load(&sim_data::find_data_from(std::path::Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap())
            .unwrap()
            .vehicle("bulwark_a1")
            .unwrap()
            .mobility
            .fuel_capacity_l
            .get();
    for k in 0..4 {
        let f = w.mobility[k].as_ref().unwrap().fuel_l;
        assert!(f > cap - 25.0, "tank {k} has {f:.0} of {cap:.0} l");
    }
    for (k, &at) in start.iter().enumerate() {
        let (limit, what) = if k < 4 { (5.0, "tank stayed in its position") } else { (15.0, "truck is back home") };
        assert!(dist(w.transform[k].pos, at) < limit, "{k}: {what}");
    }
    // 10 rounds a tank at 3 a minute, tanks one after another for the cargo truck.
    assert!((600..2000).contains(&s), "finished after {s} s");
}

/// Plan §5: fired on, the trucks stop and withdraw; the platoon reports a partial resupply.
#[test]
fn trucks_under_fire_withdraw_and_the_resupply_is_partial() {
    let (mut sim, tanks, trucks, _) = depleted_platoon();
    sim.submit(SideId(0), Command::Tailgate { platoon: tanks, trucks });
    // Until the first tank is being served.
    for _ in 0..1200 {
        run(&mut sim, 1.0);
        if sim.perceived(SideId(0)).own.iter().any(|u| u.service == Some(sim::view::ServiceStage::Served)) {
            break;
        }
    }
    sim.truth_mut().suppression[4] = 0.3; // a near miss
    let ev = run(&mut sim, 5.0);
    let rep = |r: fn(&PlatoonReport) -> bool| {
        ev.iter().any(|e| matches!(e, Event::PlatoonReport { report, .. } if r(report)))
    };
    assert!(rep(|r| matches!(r, PlatoonReport::TrucksWithdrawing)), "{ev:?}");
    assert!(rep(|r| matches!(r, PlatoonReport::Resupplied { complete: false })), "{ev:?}");
    let task = |sim: &Sim, id| sim.perceived(SideId(0)).platoons.iter().find(|p| p.id == id).unwrap().task.clone();
    assert_eq!(task(&sim, trucks), "WITHDRAWING");
}

/// A new order to the served platoon ends its resupply; the trucks go home.
#[test]
fn a_new_order_to_the_served_platoon_sends_the_trucks_home_and_replays() {
    let (mut sim, tanks, trucks, sc) = depleted_platoon();
    sim.submit(SideId(0), Command::Tailgate { platoon: tanks, trucks });
    run(&mut sim, 120.0);
    let to = [600.0, 900.0];
    sim.submit(
        SideId(0),
        Command::PlatoonMove {
            platoon: tanks,
            destination: to,
            via: vec![],
            formation: None,
            technique: sim_data::Technique::Traveling,
            speed_limit_mps: None,
        },
    );
    run(&mut sim, 30.0); // over the radio
    let task = |sim: &Sim, id| sim.perceived(SideId(0)).platoons.iter().find(|p| p.id == id).unwrap().task.clone();
    assert_eq!(task(&sim, trucks), "RETURNING");
    run(&mut sim, 60.0);
    // The run replays to the same state (the depletion is set-up, redone before playing).
    let rec = Replay::from_ron(&Replay::record(&sim).to_ron()).unwrap();
    let ds = data();
    let m = map(&ds, 600, 600, "cropland", |_, _| 0.0);
    let mut again = Sim::new(&sc, ds, m).unwrap();
    deplete_tanks(&mut again);
    rec.play(&mut again, |_, _| {});
    assert_eq!(again.state_hash(), rec.final_hash, "replay diverged");
}
