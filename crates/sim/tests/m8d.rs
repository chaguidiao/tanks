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
            if prev[1] < 800.0 && p[1] >= 800.0 {
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
    assert!((780.0..800.0).contains(&sy), "the squad is held at the wire: {sy:.1}");
    assert!(ev.iter().any(|e| matches!(e, Event::ObstacleMet { unit, .. } if *unit == squad)));
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
