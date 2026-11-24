//! M8d tests: obstacles, mines and breaching (docs/plans/M8D-ENGINEERS.md).

mod common;

use ai::picture::PlatoonId;
use common::*;
use sim::{Command, Event, SideId, Sim};
use sim_data::{Formation, ObstacleDef, ScenarioDef, Technique};

const BLUE: SideId = SideId(0);
const RED: SideId = SideId(1);
const P1: PlatoonId = PlatoonId(1);

fn obstacle(kind: &str, points: &[[f64; 2]]) -> ObstacleDef {
    ObstacleDef { kind: kind.into(), points: points.to_vec(), width_m: None, height_m: None, mines: vec![] }
}

fn minefield(points: &[[f64; 2]], density: f64) -> ObstacleDef {
    ObstacleDef { mines: vec![("at_pressure".into(), density)], ..obstacle("minefield", points) }
}

/// One Blue tank (or a platoon of `n`) at the south of a 1.6 × 2.4 km cropland map, one Red tank
/// far away to the north-east; `edit` places obstacles.
fn setup(n: usize, edit: impl FnOnce(&mut ScenarioDef)) -> Sim {
    let ds = data();
    let m = map(&ds, 400, 600, "cropland", |_, _| 0.0);
    let blue: Vec<_> = (0..n)
        .map(|i| unit(&format!("1-{}", i + 1), "bulwark_a1", [700.0 + i as f64 * 40.0, 300.0], 0.0, Some("1")))
        .collect();
    let red = vec![unit("211", "kurgan_b", [1500.0, 2300.0], 180.0, Some("21"))];
    let mut sc = scenario(10 * 3600, blue, red);
    edit(&mut sc);
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    sim.set_ai_enabled(BLUE, false);
    sim.set_ai_enabled(RED, false);
    sim
}

fn drive(sim: &mut Sim, to: [f64; 2]) {
    let unit = sim.truth().id[0];
    sim.submit(BLUE, Command::Move { unit, destination: to, via: vec![], speed_limit_mps: None });
}

fn pmove(to: [f64; 2]) -> Command {
    Command::PlatoonMove {
        platoon: P1,
        destination: to,
        via: vec![],
        formation: Some(Formation::Column),
        technique: Technique::Traveling,
        speed_limit_mps: None,
    }
}

#[test]
fn an_unknown_ditch_stops_a_tank_at_its_near_edge_and_becomes_known() {
    // FM 90-7 / USMC TBS: an anti-tank ditch wider than the tank's trench crossing stops it.
    let mut sim = setup(1, |sc| sc.sides[1].obstacles.push(obstacle("ditch", &[[100.0, 800.0], [1500.0, 800.0]])));
    assert!(sim.perceived(BLUE).obstacles.is_empty(), "Blue does not know Red's ditch");
    assert_eq!(sim.perceived(RED).obstacles.len(), 1, "Red knows its own");
    drive(&mut sim, [700.0, 1500.0]);
    let ev = run(&mut sim, 180.0);
    let y = sim.truth().transform[0].pos[1];
    assert!((770.0..798.0).contains(&y), "halted short of the ditch at y 798: {y:.1}");
    assert!(ev.iter().any(|e| matches!(e, Event::ObstacleMet { obstacle: 0, .. })), "reports the obstacle");
    let seen = &sim.perceived(BLUE).obstacles;
    assert!(seen.len() == 1 && seen[0].kind == "ditch" && !seen[0].own, "{seen:?}");
}

#[test]
fn a_known_wall_is_planned_round() {
    // Blue's own dragon's teeth across most of the map, open at the east end: a platoon ordered
    // north plans round them (decision #2: a side plans round what it knows).
    let mut sim = setup(4, |sc| sc.sides[0].obstacles.push(obstacle("teeth", &[[0.0, 800.0], [1200.0, 800.0]])));
    sim.submit(BLUE, pmove([700.0, 1400.0]));
    let mut crossed_x: Vec<f64> = Vec::new();
    let mut last: Vec<[f64; 2]> = (0..4).map(|k| sim.truth().transform[k].pos).collect();
    for _ in 0..600 {
        run(&mut sim, 1.0);
        for (k, prev) in last.iter_mut().enumerate() {
            let p = sim.truth().transform[k].pos;
            if prev[1] < 800.0 && p[1] >= 800.0 && crossed_x.len() < 4 {
                crossed_x.push(p[0]);
            }
            *prev = p;
        }
    }
    let at: Vec<[f64; 2]> = (0..4).map(|k| sim.truth().transform[k].pos).collect();
    assert_eq!(
        crossed_x.len(),
        4,
        "every tank got past the line: {crossed_x:?} {at:?} {:?}",
        sim.perceived(BLUE).platoons[0].task
    );
    assert!(crossed_x.iter().all(|x| *x > 1200.0), "all through the gap: {crossed_x:?}");
    let ys: Vec<f64> = (0..4).map(|k| sim.truth().transform[k].pos[1]).collect();
    assert!(ys.iter().all(|y| *y > 1100.0), "arrived (a column stacks back from the leader): {ys:?}");
}

#[test]
fn wire_slows_a_tank_and_stops_men() {
    let mut sim = setup(1, |sc| {
        sc.sides[1].obstacles.push(obstacle("wire", &[[100.0, 800.0], [1500.0, 800.0]]));
        sc.sides[0].units.push(squad("1-1A", "rifle_squad_b", [900.0, 700.0], Some("9"), None));
    });
    drive(&mut sim, [700.0, 1200.0]);
    let squad = sim.truth().id[1];
    sim.submit(BLUE, Command::Move { unit: squad, destination: [900.0, 1000.0], via: vec![], speed_limit_mps: None });
    let ev = run(&mut sim, 300.0);
    let w = sim.truth();
    assert!(w.transform[0].pos[1] > 1100.0, "the tank crushed through: {:.0}", w.transform[0].pos[1]);
    let sy = w.transform[1].pos[1];
    assert!((740.0..800.0).contains(&sy), "the squad is held short of the wire: {sy:.1}");
    let _ = ev;
}

#[test]
fn a_dense_minefield_strikes_and_becomes_known() {
    // Five pressure mines per metre of front: about 99 % encounter (FM 20-32 ch. 2-3).
    let mut sim = setup(1, |sc| sc.sides[1].obstacles.push(minefield(&[[100.0, 800.0], [1500.0, 800.0]], 5.0)));
    drive(&mut sim, [700.0, 1200.0]);
    let ev = run(&mut sim, 200.0);
    let strike = ev.iter().find_map(|e| match e {
        Event::MineStrike { kit: false, kill, .. } => Some(*kill),
        _ => None,
    });
    assert!(strike.is_some(), "a strike crossing the field");
    let y = sim.truth().transform[0].pos[1];
    assert!((745.0..860.0).contains(&y), "stopped in the field: {y:.0}");
    assert_eq!(sim.perceived(BLUE).obstacles.len(), 1, "the strike reveals the field");
}

#[test]
fn obstacles_change_the_state_hash_only_when_present() {
    let a = setup(1, |_| {}).state_hash();
    let b = setup(1, |_| {}).state_hash();
    let c =
        setup(1, |sc| sc.sides[1].obstacles.push(obstacle("wire", &[[100.0, 800.0], [1500.0, 800.0]]))).state_hash();
    assert_eq!(a, b);
    assert_ne!(a, c);
}

fn reports(ev: &[Event]) -> Vec<sim::PlatoonReport> {
    ev.iter()
        .filter_map(|e| match e {
            Event::PlatoonReport { side: BLUE, report, .. } => Some(report.clone()),
            _ => None,
        })
        .collect()
}

fn ys(sim: &Sim, n: usize) -> Vec<f64> {
    (0..n).map(|k| sim.truth().transform[k].pos[1]).collect()
}

#[test]
fn a_platoon_sees_an_obstacle_and_bypasses_it() {
    // Red dragon's teeth across most of the map, open at the east end. Blue does not know of
    // them until it sees them, then re-plans round (default ON OBSTACLE: BYPASS).
    let mut sim = setup(4, |sc| sc.sides[1].obstacles.push(obstacle("teeth", &[[0.0, 800.0], [1200.0, 800.0]])));
    sim.submit(BLUE, pmove([700.0, 1400.0]));
    let ev = run(&mut sim, 600.0);
    assert!(ev.iter().any(|e| matches!(e, Event::ObstacleSeen { side: BLUE, obstacle: 0 })), "seen");
    let r = reports(&ev);
    assert!(
        r.iter()
            .any(|r| matches!(r, sim::PlatoonReport::Obstacle { reaction: ai::picture::ObstacleReaction::Bypass, .. })),
        "{r:?}"
    );
    let y = ys(&sim, 4);
    assert!(y.iter().all(|y| *y > 1100.0), "round and on: {y:?}");
}

#[test]
fn with_no_bypass_the_platoon_holds_short_and_says_so() {
    let mut sim = setup(4, |sc| sc.sides[1].obstacles.push(obstacle("teeth", &[[0.0, 800.0], [1600.0, 800.0]])));
    sim.submit(BLUE, pmove([700.0, 1400.0]));
    let ev = run(&mut sim, 300.0);
    let r = reports(&ev);
    assert!(r.contains(&sim::PlatoonReport::NoBypass), "{r:?}");
    assert!(ys(&sim, 4).iter().all(|y| *y < 790.0));
    assert_eq!(sim.perceived(BLUE).platoons[0].task, "HALTED AT OBSTACLE");
}

#[test]
fn a_plough_tank_breaches_a_minefield_and_the_platoon_goes_through_the_lane() {
    // FM 20-32 ch. 10: the plough starts 100 m short and ploughs 100 m past the far edge; the
    // platoon follows in column through the 4.5 m lane.
    let mut sim = setup(4, |sc| {
        sc.sides[1].obstacles.push(ObstacleDef {
            mines: vec![("at_pressure".into(), 0.6), ("at_full_width".into(), 0.3)],
            ..obstacle("minefield", &[[0.0, 800.0], [1600.0, 800.0]])
        });
        sc.sides[0].units[0].kit = Some("plough_b".into());
    });
    // Blue knows the field (reported by recce): it plans to breach rather than bypass.
    sim.submit_immediate(
        BLUE,
        Command::SetObstacleReaction { platoon: P1, reaction: ai::picture::ObstacleReaction::Breach },
    );
    run(&mut sim, 1.0);
    sim.submit(BLUE, pmove([700.0, 1400.0]));
    let ev = run(&mut sim, 900.0);
    let r = reports(&ev);
    assert!(r.iter().any(|r| matches!(r, sim::PlatoonReport::Breaching { .. })), "{r:?}");
    assert!(r.iter().any(|r| matches!(r, sim::PlatoonReport::LaneOpen { .. })), "{r:?}");
    let lanes = &sim.perceived(BLUE).obstacles[0].lanes;
    assert!(lanes.len() == 1 && !lanes[0].open && (lanes[0].width_m - 4.5).abs() < 1e-9, "{lanes:?}");
    let through = ys(&sim, 4).iter().filter(|y| **y > 900.0).count();
    assert!(through >= 3, "through the lane: {:?} {:?}", ys(&sim, 4), sim.truth().condition);
}

#[test]
fn ordered_to_breach_without_an_asset_the_platoon_holds_and_engineers_open_the_way() {
    // Red abatis across the road: closed to tanks, cut by engineers by hand (about 15 min). The
    // tank platoon (BREACH, but no plough) holds short; the engineers' lane lets it go on.
    let mut sim = setup(4, |sc| {
        sc.sides[1].obstacles.push(obstacle("abatis", &[[0.0, 800.0], [1600.0, 800.0]]));
        for i in 0..3 {
            sc.sides[0].units.push(squad(
                &format!("E-{}", i + 1),
                "engineer_squad_b",
                [600.0 + 60.0 * i as f64, 650.0],
                Some("E"),
                None,
            ));
        }
    });
    let eng = PlatoonId(2);
    for p in [P1, eng] {
        sim.submit_immediate(
            BLUE,
            Command::SetObstacleReaction { platoon: p, reaction: ai::picture::ObstacleReaction::Breach },
        );
    }
    run(&mut sim, 1.0);
    sim.submit(BLUE, pmove([700.0, 1400.0]));
    sim.submit(
        BLUE,
        Command::PlatoonMove {
            platoon: eng,
            destination: [650.0, 1000.0],
            via: vec![],
            formation: None,
            technique: Technique::Traveling,
            speed_limit_mps: None,
        },
    );
    let ev = run(&mut sim, 1800.0);
    let r = reports(&ev);
    assert!(r.contains(&sim::PlatoonReport::NoBreachAsset), "{r:?}");
    assert!(r.iter().any(|r| matches!(r, sim::PlatoonReport::LaneOpen { .. })), "{r:?}");
    let y = ys(&sim, 4);
    assert!(y.iter().all(|y| *y > 1000.0), "the tanks went on through the engineers' lane: {y:?}");
}

#[test]
fn halt_on_obstacle_waits_for_orders() {
    let mut sim = setup(4, |sc| sc.sides[1].obstacles.push(obstacle("teeth", &[[0.0, 800.0], [1600.0, 800.0]])));
    sim.submit_immediate(
        BLUE,
        Command::SetObstacleReaction { platoon: P1, reaction: ai::picture::ObstacleReaction::Halt },
    );
    run(&mut sim, 1.0);
    sim.submit(BLUE, pmove([700.0, 1400.0]));
    let ev = run(&mut sim, 400.0);
    let r = reports(&ev);
    assert!(
        r.iter()
            .any(|r| matches!(r, sim::PlatoonReport::Obstacle { reaction: ai::picture::ObstacleReaction::Halt, .. })),
        "{r:?}"
    );
    assert!(!r.contains(&sim::PlatoonReport::NoBypass), "it did not look for a bypass: {r:?}");
    assert!(ys(&sim, 4).iter().all(|y| *y < 790.0), "holds short");
    assert_eq!(sim.perceived(BLUE).platoons[0].obstacle_reaction, ai::picture::ObstacleReaction::Halt);
}

#[test]
fn a_platoon_does_not_drive_through_its_own_minefield() {
    let mut sim = setup(4, |sc| sc.sides[0].obstacles.push(minefield(&[[0.0, 800.0], [1600.0, 800.0]], 0.8)));
    sim.submit(BLUE, pmove([700.0, 1400.0]));
    let ev = run(&mut sim, 400.0);
    let r = reports(&ev);
    assert!(ys(&sim, 4).iter().all(|y| *y < 760.0), "{:?} {r:?}", ys(&sim, 4));
}

#[test]
fn a_platoon_halted_at_a_minefield_breaches_when_told_to() {
    // Default BYPASS: no way round, so it holds; ON OBSTACLE BREACH then sends the plough in.
    let mut sim = setup(4, |sc| {
        sc.sides[1].obstacles.push(minefield(&[[0.0, 800.0], [1600.0, 800.0]], 0.8));
        sc.sides[0].units[0].kit = Some("plough_b".into());
        sc.sides[0].obstacles.push(obstacle("wire", &[[0.0, 2300.0], [10.0, 2300.0]])); // Blue knows something
    });
    // Recce has reported the field.
    sim.submit(BLUE, pmove([700.0, 1400.0]));
    let mut ev = run(&mut sim, 240.0);
    sim.submit(BLUE, Command::SetObstacleReaction { platoon: P1, reaction: ai::picture::ObstacleReaction::Breach });
    ev.extend(run(&mut sim, 600.0));
    let r = reports(&ev);
    assert!(r.iter().any(|r| matches!(r, sim::PlatoonReport::LaneOpen { .. })), "{r:?}");
    assert!(ys(&sim, 4).iter().filter(|y| **y > 900.0).count() >= 3, "{:?}", ys(&sim, 4));
}

/// 1.6 × 2.4 km: a river west to east at y 1200, roads north to south at x 500 and x 1300 (two
/// bridges). Blue's platoon at the south end of the west road.
fn river_setup(edit: impl FnOnce(&mut ScenarioDef)) -> Sim {
    use terrain::features::{River, RiverProps, Road, RoadClass, RoadProps};
    let ds = data();
    let m = map_with(&ds, 400, 600, "grassland", |p| {
        p.features.rivers.push(River {
            id: 1,
            points: vec![[0.0, 1200.0], [1600.0, 1200.0]],
            props: RiverProps { width_m: vec![30.0, 30.0], depth_m: vec![3.0, 3.0] },
        });
        for (id, x) in [(2, 500.0), (3, 1300.0)] {
            p.features.roads.push(Road {
                id,
                points: vec![[x, 0.0], [x, 2400.0]],
                props: RoadProps { class: RoadClass::Secondary, width_m: 6.0 },
            });
        }
    });
    assert_eq!(m.bridges.len(), 2);
    let blue: Vec<_> = (0..4)
        .map(|i| unit(&format!("1-{}", i + 1), "bulwark_a1", [480.0 + i as f64 * 15.0, 300.0], 0.0, Some("1")))
        .collect();
    let red = vec![unit("211", "kurgan_b", [1500.0, 2300.0], 180.0, Some("21"))];
    let mut sc = scenario(10 * 3600, blue, red);
    edit(&mut sc);
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    sim.set_ai_enabled(BLUE, false);
    sim.set_ai_enabled(RED, false);
    sim
}

#[test]
fn a_blown_bridge_closes_the_crossing_and_the_platoon_takes_the_other() {
    // Red blows the west bridge at once (FM 90-7 reserved demolition); Blue sees the dropped span
    // and goes round by the east bridge.
    let mut sim = river_setup(|sc| {
        sc.sides[1].demolitions.push(sim_data::DemolitionDef {
            at: [500.0, 1200.0],
            at_s: Some(1.0),
            code_word: None,
            enemy_within_m: None,
        });
    });
    sim.submit(BLUE, pmove([500.0, 2000.0]));
    let mut crossed_x = None;
    let mut ev = Vec::new();
    for _ in 0..900 {
        let before = sim.truth().transform[0].pos;
        ev.extend(run(&mut sim, 1.0));
        let after = sim.truth().transform[0].pos;
        if before[1] < 1200.0 && after[1] >= 1200.0 {
            crossed_x = Some(after[0]);
        }
    }
    assert!(ev.iter().any(|e| matches!(e, Event::BridgeBlown { side: RED, .. })));
    assert!(ev.iter().any(|e| matches!(e, Event::ObstacleSeen { side: BLUE, .. })), "the span is seen");
    let x = crossed_x.expect("the leader crossed the river");
    assert!((x - 1300.0).abs() < 40.0, "by the east bridge: {x:.0}");
}

#[test]
fn an_enemy_close_to_a_prepared_bridge_sets_it_off() {
    // Blown when Red sees an enemy within 400 m of the bridge (its observer looks down the road).
    let mut sim = river_setup(|sc| {
        sc.sides[1].demolitions.push(sim_data::DemolitionDef {
            at: [500.0, 1200.0],
            at_s: None,
            code_word: None,
            enemy_within_m: Some(400.0),
        });
        sc.sides[1].units.push(unit("231", "kurgan_b", [560.0, 1700.0], 180.0, Some("23")));
    });
    sim.submit_immediate(
        BLUE,
        Command::SetContactReaction { platoon: P1, reaction: ai::picture::ContactReaction::Continue },
    );
    // Blue holds its fire, so Red's observer lives to report it.
    sim.submit_immediate(
        BLUE,
        Command::WeaponsControl { to: sim::Addressee::Platoon(P1), status: ai::picture::WeaponsStatus::Hold },
    );
    run(&mut sim, 1.0);
    sim.submit(BLUE, pmove([500.0, 2000.0]));
    let ev = run(&mut sim, 600.0);
    assert!(ev.iter().any(|e| matches!(e, Event::BridgeBlown { side: RED, .. })), "blown");
    assert_eq!(sim.perceived(RED).obstacles.iter().filter(|o| o.kind == "blown_bridge").count(), 1);
}
