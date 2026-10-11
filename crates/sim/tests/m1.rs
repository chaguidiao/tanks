//! M1 acceptance tests on small synthetic maps: mobility on terrain, Move order, routing,
//! determinism.

use std::path::PathBuf;
use std::sync::Arc;

use sim::mobility::power_limited_speed;
use sim::{Command, EntityId, Event, SideId, Sim};
use sim_data::{DataSet, MapRef, ScenarioDef, SideDef, UnitDef};
use terrain::features::{River, RiverProps, Road, RoadClass, RoadProps};
use terrain::mobility::MobilityClass;
use terrain::{AuthoringPackage, BakedMap, MapManifest, bake};

fn data() -> Arc<DataSet> {
    Arc::new(DataSet::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")).unwrap())
}

fn flat_pkg(w: usize, h: usize, landcover: &str, soil: &str, ds: &DataSet) -> AuthoringPackage {
    let m = MapManifest {
        id: "synthetic".into(),
        name: "synthetic".into(),
        width_cells: w,
        height_cells: h,
        cell_size_m: 4.0,
        seed: 1,
        recipe: None,
    };
    let mut p = AuthoringPackage::new_flat(m, 100.0);
    p.landcover.data.fill(ds.terrain.landcover_id(landcover).unwrap());
    p.soil.data.fill(ds.terrain.soils.iter().find(|s| s.name == soil).unwrap().id);
    p
}

fn baked(pkg: &AuthoringPackage, ds: &DataSet) -> Arc<BakedMap> {
    let classes: Vec<MobilityClass> = ds.vehicles.iter().map(MobilityClass::from).collect();
    Arc::new(bake(pkg, &ds.terrain, &classes))
}

fn scenario(year: i32, pos: [f64; 2], heading_deg: f64) -> ScenarioDef {
    ScenarioDef {
        id: "t".into(),
        name: "t".into(),
        date: (year, 6, 1),
        start_time_s: 6 * 3600,
        map: MapRef { id: "synthetic".into(), recipe: "mixed-test".into(), seed: 1, size_m: 0.0 },
        seed: 99,
        sides: vec![SideDef {
            name: "BLUE".into(),
            doctrine: "western-combined".into(),
            tech_year: None,
            units: vec![UnitDef {
                callsign: "1-1".into(),
                vehicle: "bulwark_a1".into(),
                position: pos,
                heading_deg,
                platoon: None,
            }],
        }],
    }
}

const BLUE: SideId = SideId(0);
const TANK: EntityId = EntityId(1);

fn run(sim: &mut Sim, seconds: f64) -> Vec<Event> {
    let mut ev = Vec::new();
    for _ in 0..(seconds * sim::TICK_HZ as f64) as usize {
        ev.extend(sim.step());
    }
    ev
}

#[test]
fn reaches_governed_speed_on_a_paved_road() {
    let ds = data();
    let map = baked(&flat_pkg(1000, 40, "road", "paved", &ds), &ds);
    let mut sim = Sim::new(&scenario(1990, [20.0, 80.0], 90.0), ds.clone(), map).unwrap();
    sim.submit_immediate(BLUE, Command::Move { unit: TANK, destination: [3900.0, 80.0], speed_limit_mps: None });
    run(&mut sim, 40.0);
    let v = sim.perceived(BLUE).unit(TANK).unwrap().speed;
    let governed = ds.vehicle("bulwark_a1").unwrap().mobility.max_speed_road_mps.get();
    assert!((v - governed).abs() < 0.2, "speed {v} vs governed {governed}");
}

#[test]
fn climb_speed_matches_power_limited_formula() {
    // Constant 30 % grade northwards on firm loam. Steady state must satisfy
    // η·P / v = m·g·(f·cosθ + sinθ) (Wong, Theory of Ground Vehicles, tracked-vehicle performance).
    let ds = data();
    let mut pkg = flat_pkg(60, 400, "grassland", "loam", &ds);
    for j in 0..400 {
        for i in 0..60 {
            pkg.elevation_base.set(i, j, (100.0 + 0.3 * (j as f64 + 0.5) * 4.0) as f32);
        }
    }
    let map = baked(&pkg, &ds);
    let mut sim = Sim::new(&scenario(1990, [120.0, 40.0], 0.0), ds.clone(), map).unwrap();
    sim.submit_immediate(BLUE, Command::Move { unit: TANK, destination: [120.0, 1550.0], speed_limit_mps: None });
    run(&mut sim, 60.0);
    let v = sim.perceived(BLUE).unit(TANK).unwrap().speed;
    let m = &ds.vehicle("bulwark_a1").unwrap().mobility;
    let loam = ds.terrain.soils.iter().find(|s| s.name == "loam").unwrap();
    let want = power_limited_speed(
        m.engine_power_w.get(),
        m.drivetrain_efficiency.get(),
        m.combat_mass_kg.get(),
        loam.rolling_resistance.get(),
        0.3,
    );
    assert!((v - want).abs() / want < 0.05, "climb speed {v:.2} m/s, formula {want:.2} m/s");
}

#[test]
fn slope_beyond_limit_has_no_route() {
    let ds = data();
    let mut pkg = flat_pkg(60, 200, "grassland", "loam", &ds);
    for j in 0..200 {
        for i in 0..60 {
            // 80 % wall between y = 300 m and 500 m.
            let y = (j as f64 + 0.5) * 4.0;
            let z = 100.0 + 0.8 * (y.clamp(300.0, 500.0) - 300.0);
            pkg.elevation_base.set(i, j, z as f32);
        }
    }
    let map = baked(&pkg, &ds);
    let mut sim = Sim::new(&scenario(1990, [120.0, 100.0], 0.0), ds, map).unwrap();
    sim.submit_immediate(BLUE, Command::Move { unit: TANK, destination: [120.0, 700.0], speed_limit_mps: None });
    let ev = run(&mut sim, 1.0);
    assert!(ev.contains(&Event::NoRoute { unit: TANK }), "{ev:?}");
}

fn river_map(ds: &DataSet) -> Arc<BakedMap> {
    // 1.2 km square of grass; a 14 m wide, 2.5 m deep river runs west-east at y = 600 m; a
    // track crosses it north-south at x = 900 m (the only crossing).
    let mut pkg = flat_pkg(300, 300, "grassland", "loam", ds);
    let ys = 600.0;
    pkg.features.rivers.push(River {
        id: 1,
        points: vec![[0.0, ys], [1200.0, ys]],
        props: RiverProps { width_m: vec![14.0, 14.0], depth_m: vec![2.5, 2.5] },
    });
    pkg.features.roads.push(Road {
        id: 2,
        points: vec![[900.0, 0.0], [900.0, 1200.0]],
        props: RoadProps { class: RoadClass::Track, width_m: 4.0 },
    });
    baked(&pkg, ds)
}

#[test]
fn move_order_routes_over_the_bridge_and_arrives() {
    let ds = data();
    let map = river_map(&ds);
    let mut sim = Sim::new(&scenario(1990, [200.0, 200.0], 0.0), ds.clone(), map.clone()).unwrap();
    sim.submit_immediate(BLUE, Command::Move { unit: TANK, destination: [200.0, 1000.0], speed_limit_mps: None });
    let ford = ds.vehicle("bulwark_a1").unwrap().mobility.fording_depth_m.get();
    let mut arrived_at = None;
    let mut crossed_x = None;
    for t in 0..(400 * sim::TICK_HZ) {
        let ev = sim.step();
        let p = sim.truth().transform[0].pos;
        let (i, j) = map.elevation.cell_of(p[0], p[1]);
        assert!(map.water_depth.get(i, j) as f64 <= ford, "drove into deep water at {p:?} (t = {t})");
        if crossed_x.is_none() && p[1] > 600.0 {
            crossed_x = Some(p[0]);
        }
        if ev.contains(&Event::Arrived { unit: TANK }) {
            arrived_at = Some(t as f64 / sim::TICK_HZ as f64);
            break;
        }
    }
    let x = crossed_x.expect("never crossed the river");
    assert!((x - 900.0).abs() < 8.0, "crossed at x = {x:.1}, bridge at 900");
    let t = arrived_at.expect("did not arrive within 400 s");
    // ~700 m to the bridge + ~700 m back west + 800 m north ≈ 2 km at ≤ 13.4 m/s cross-country.
    assert!(t < 300.0, "took {t:.0} s");
    let u = sim.perceived(BLUE).unit(TANK).unwrap();
    assert!(u.fuel_fraction < 1.0 && u.fuel_fraction > 0.9);
    assert!((u.pos[0] - 200.0).abs() < 6.0 && (u.pos[1] - 1000.0).abs() < 6.0);
}

#[test]
fn vehicle_must_be_in_service_at_scenario_date() {
    let ds = data();
    let map = baked(&flat_pkg(50, 50, "grassland", "loam", &ds), &ds);
    let err = Sim::new(&scenario(1975, [20.0, 20.0], 0.0), ds, map).err().expect("1975 is before in-service date");
    assert!(err.contains("not in service"), "{err}");
}

fn replay_hash(ds: &Arc<DataSet>, map: &Arc<BakedMap>) -> (u64, Vec<Event>) {
    let mut sim = Sim::new(&scenario(1990, [200.0, 200.0], 45.0), ds.clone(), map.clone()).unwrap();
    let mut events = Vec::new();
    for t in 0..(120 * sim::TICK_HZ) {
        if t == 10 {
            sim.submit_immediate(
                BLUE,
                Command::Move { unit: TANK, destination: [1000.0, 1000.0], speed_limit_mps: None },
            );
        }
        if t == 900 {
            sim.submit_immediate(
                BLUE,
                Command::Move { unit: TANK, destination: [300.0, 900.0], speed_limit_mps: Some(8.0) },
            );
        }
        if t == 2000 {
            sim.submit_immediate(BLUE, Command::Halt { to: sim::Addressee::Unit(TANK) });
        }
        events.extend(sim.step());
    }
    (sim.state_hash(), events)
}

#[test]
fn same_commands_give_the_same_state_hash() {
    let ds = data();
    let map = river_map(&ds);
    let (h1, e1) = replay_hash(&ds, &map);
    let (h2, e2) = replay_hash(&ds, &map);
    assert_eq!(h1, h2);
    assert_eq!(e1, e2);
    // Golden value: update deliberately (and say why in the commit) when behaviour changes.
    const GOLDEN: u64 = 0xbfe3_438f_0879_258e;
    assert_eq!(h1, GOLDEN, "state hash changed: {h1:#018x}");
}
