//! M4 acceptance tests: platoon formations, techniques, battle positions, fire distribution,
//! react-to-contact drill.

mod common;

use ai::position::{BpQuery, observers};
use common::*;
use sim::view::{Condition, PlatoonId, WeaponsStatus};
use sim::{Addressee, Command, EntityId, Event, PlatoonReport, PositionKind, SideId, Sim};
use sim_data::{Formation, Technique};
use terrain::ground::Ground;
use terrain::los::sight;

const BLUE: SideId = SideId(0);
const RED: SideId = SideId(1);
const P1: PlatoonId = PlatoonId(1);
const DAY: u32 = 10 * 3600;

fn platoon(y: f64) -> Vec<sim_data::UnitDef> {
    (0..4).map(|i| unit(&format!("1-{}", i + 1), "bulwark_a1", [250.0 + i as f64 * 100.0, y], 0.0, Some("1"))).collect()
}

#[test]
fn occupy_bp_finds_hull_down_positions_within_120_s() {
    // East-west ridge (crest at y = 1000 m, 12 m high); threat to the north. The BP is on the
    // reverse (south) slope. DESIGN.md §12: "all four tanks hull-down vs. threat within 120 s".
    let ds = data();
    let m = map(&ds, 200, 1000, "cropland", |_, y| 12.0 * (-((y - 1000.0) / 150.0).powi(2)).exp());
    let sc = scenario(DAY, platoon(600.0), vec![]);
    let mut sim = Sim::new(&sc, ds.clone(), m.clone()).unwrap();
    sim.submit_immediate(
        BLUE,
        Command::OccupyPosition {
            platoon: P1,
            centre: [400.0, 920.0],
            radius_m: 200.0,
            facing: 0.0,
            kind: PositionKind::HullDown,
        },
    );
    let events = run(&mut sim, 120.0);
    let set = events.iter().find_map(|e| match e {
        Event::PlatoonReport { report: PlatoonReport::SetInPosition { hull_down, .. }, .. } => Some(*hull_down),
        _ => None,
    });
    assert_eq!(set, Some(4), "platoon reports four hull-down positions");
    // Independent check from the threat observers.
    let g = Ground::new(&m, &ds.terrain);
    let v = ds.vehicle("bulwark_a1").unwrap();
    let q = BpQuery {
        centre: [400.0, 920.0],
        radius: 200.0,
        facing: 0.0,
        hull_height: v.dimensions.hull_height_m.get(),
        sight_height: v.dimensions.sight_height_m.get(),
        gun_depression: 0.17,
        turret_down: false,
    };
    let obs = observers(&q, m.manifest.width_m(), m.manifest.height_m());
    let w = sim.truth();
    for k in 0..4 {
        let p = w.transform[k].pos;
        let mut sight_seen = 0;
        for o in &obs {
            let s = sight(&g, *o, 2.4, p, &[q.hull_height * 0.95, q.sight_height]);
            assert!(!s.visible[0], "tank {k} at {p:?}: hull visible from {o:?}");
            if s.visible[1] {
                sight_seen += 1;
            }
        }
        assert!(sight_seen * 2 >= obs.len(), "tank {k}: sight must cover the threat ({sight_seen}/{})", obs.len());
        assert!(dist(p, [400.0, 920.0]) <= 205.0);
    }
    // Dispersed: no two tanks closer than 50 m.
    for a in 0..4 {
        for b in a + 1..4 {
            assert!(dist(w.transform[a].pos, w.transform[b].pos) > 50.0);
        }
    }
}

#[test]
fn wedge_move_keeps_formation_and_reports_set() {
    let ds = data();
    let m = map(&ds, 250, 750, "grassland", |_, _| 0.0);
    let mut sim = Sim::new(&scenario(DAY, platoon(300.0), vec![]), ds, m).unwrap();
    sim.submit_immediate(
        BLUE,
        Command::PlatoonMove {
            platoon: P1,
            destination: [500.0, 2300.0],
            formation: Some(Formation::Wedge),
            technique: Technique::Traveling,
        },
    );
    run(&mut sim, 90.0);
    // Mid-move: a wedge is wider than a column and the leader is ahead.
    let w = sim.truth();
    let xs: Vec<f64> = (0..4).map(|k| w.transform[k].pos[0]).collect();
    let spread = xs.iter().cloned().fold(f64::MIN, f64::max) - xs.iter().cloned().fold(f64::MAX, f64::min);
    assert!(spread > 150.0, "wedge lateral spread {spread:.0} m");
    let leader_y = w.transform[0].pos[1];
    assert!((1..4).all(|k| w.transform[k].pos[1] < leader_y), "leader leads");
    let events = run(&mut sim, 300.0);
    assert!(events.iter().any(|e| matches!(e, Event::PlatoonReport { report: PlatoonReport::SetAt { .. }, .. })));
    let w = sim.truth();
    for k in 0..4 {
        assert!(dist(w.transform[k].pos, [500.0, 2300.0]) < 350.0);
    }
}

#[test]
fn bounding_overwatch_alternates_sections() {
    let ds = data();
    let m = map(&ds, 250, 750, "grassland", |_, _| 0.0);
    let mut sim = Sim::new(&scenario(DAY, platoon(300.0), vec![]), ds, m).unwrap();
    sim.submit_immediate(
        BLUE,
        Command::PlatoonMove {
            platoon: P1,
            destination: [500.0, 2600.0],
            formation: Some(Formation::Line),
            technique: Technique::BoundingOverwatch,
        },
    );
    let mut saw_split = false;
    let mut bounds = 0;
    for _ in 0..600 {
        for e in run(&mut sim, 1.0) {
            if matches!(e, Event::PlatoonReport { report: PlatoonReport::Bounding { .. }, .. }) {
                bounds += 1;
            }
        }
        let w = sim.truth();
        let moving = |k: usize| w.mobility[k].as_ref().unwrap().speed > 2.0;
        if (moving(0) && moving(1) && !moving(2) && !moving(3)) || (!moving(0) && !moving(1) && moving(2) && moving(3))
        {
            saw_split = true;
        }
    }
    assert!(saw_split, "one section moves while the other overwatches");
    assert!(bounds >= 2, "sections alternate ({bounds} bounds)");
}

#[test]
fn platoon_reacts_to_contact_and_distributes_fire() {
    // Two hostile tanks spread across the front, 2.6 km ahead; the platoon advances in line.
    let ds = data();
    let m = map(&ds, 400, 1000, "cropland", |_, _| 0.0);
    let red = vec![
        unit("201", "kurgan_b", [500.0, 3200.0], 180.0, None),
        unit("202", "kurgan_b", [1100.0, 3200.0], 180.0, None),
    ];
    let blue: Vec<_> = (0..4)
        .map(|i| unit(&format!("1-{}", i + 1), "bulwark_a1", [500.0 + i as f64 * 200.0, 300.0], 0.0, Some("1")))
        .collect();
    let mut sim = Sim::new(&scenario(DAY, blue, red), ds, m).unwrap();
    for u in [5, 6] {
        sim.submit_immediate(
            RED,
            Command::WeaponsControl { to: Addressee::Unit(EntityId(u)), status: WeaponsStatus::Hold },
        );
    }
    sim.submit_immediate(
        BLUE,
        Command::PlatoonMove {
            platoon: P1,
            destination: [800.0, 3000.0],
            formation: Some(Formation::Line),
            technique: Technique::Traveling,
        },
    );
    let events = run(&mut sim, 240.0);
    assert!(
        events.iter().any(|e| matches!(e, Event::PlatoonReport { report: PlatoonReport::Contact { .. }, .. })),
        "contact drill triggered"
    );
    let w = sim.truth();
    assert!(
        w.condition[4] != Condition::Operational && w.condition[5] != Condition::Operational,
        "both targets engaged and knocked out"
    );
    // Halted short of the enemy rather than driving into it.
    assert!((0..4).all(|k| w.transform[k].pos[1] < 2500.0));
}
