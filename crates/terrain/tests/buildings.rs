//! Building identity, entrances and the foot raster (TODO H8, H9) on a small synthetic map.

use std::path::PathBuf;

use sim_data::{BuildingClass, DataSet, TerrainData};
use terrain::features::{Building, BuildingProps};
use terrain::mobility::{FOOT_CLASS, MobilityClass};
use terrain::path::{Pathfinder, segment_time};
use terrain::{AuthoringPackage, BakedMap, MapManifest, Vec2, bake};

const CELL: f64 = 4.0;

fn td() -> TerrainData {
    DataSet::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")).unwrap().terrain
}

fn rect(id: u32, x0: f64, y0: f64, x1: f64, y1: f64, class: BuildingClass) -> Building {
    // Counter-clockwise.
    Building { id, ring: vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]], props: BuildingProps { height_m: 7.0, class } }
}

/// 400 m square of flat grassland with:
/// 1. a 12 x 12 m house (3 x 3 cells, one interior cell),
/// 2. an 8 x 12 m house (2 x 3 cells, no interior), wound clockwise,
/// 3. a 2.5 m shed between cell centres (no cells),
/// 4. and 5. a terraced pair sharing a wall.
fn map(td: &TerrainData) -> BakedMap {
    let m = MapManifest {
        id: "b".into(),
        name: "b".into(),
        width_cells: 100,
        height_cells: 100,
        cell_size_m: CELL,
        seed: 1,
        recipe: None,
    };
    let mut p = AuthoringPackage::new_flat(m, 100.0);
    p.landcover.data.fill(td.landcover_id("grassland").unwrap());
    let mut cw = rect(11, 140.0, 100.0, 148.0, 112.0, BuildingClass::Wood);
    cw.ring.reverse();
    p.features.buildings = vec![
        rect(10, 100.0, 100.0, 112.0, 112.0, BuildingClass::Masonry),
        cw,
        rect(12, 199.0, 199.0, 201.5, 201.5, BuildingClass::Wood),
        rect(13, 200.0, 300.0, 212.0, 312.0, BuildingClass::Masonry),
        rect(14, 212.0, 300.0, 224.0, 312.0, BuildingClass::Masonry),
    ];
    let vehicle = MobilityClass {
        id: "tank".into(),
        road_speed: 18.0,
        cross_country_speed: 12.0,
        max_slope: 0.5,
        fording_depth: 1.2,
        foot: false,
    };
    bake(&p, td, &[vehicle, MobilityClass::foot(&td.foot)])
}

fn speed(b: &BakedMap, class: &str, p: Vec2) -> u8 {
    let (i, j) = b.elevation.cell_of(p[0], p[1]);
    b.mobility(class).unwrap().speed.get(i, j)
}

#[test]
fn buildings_get_ids_areas_and_entrances() {
    let td = td();
    let b = map(&td);
    assert_eq!(b.buildings.len(), 5);
    let house = b.building_at(106.0, 106.0).expect("house");
    assert_eq!((house.id, house.feature_id, house.class), (1, 10, BuildingClass::Masonry));
    assert_eq!(house.cells, 9);
    assert!((house.footprint_m2 - 144.0).abs() < 1e-3);
    assert!((house.centroid[0] - 106.0).abs() < 1e-3 && (house.centroid[1] - 106.0).abs() < 1e-3);
    assert_eq!(house.entrances.len(), 4, "a free-standing house can be entered from every side");
    // Winding does not matter: the clockwise footprint gets the same outward walls.
    let cw = b.building(2).unwrap();
    assert_eq!((cw.cells, cw.entrances.len()), (6, 4));
    assert!((cw.footprint_m2 - 96.0).abs() < 1e-3);
    let shed = b.building(3).unwrap();
    assert_eq!((shed.cells, shed.entrances.len()), (0, 0), "smaller than a cell: blocks nothing");
    assert!(b.building_at(200.0, 200.0).is_none());
    // The shared wall of the terraced pair is not an entrance.
    assert_eq!(b.building(4).unwrap().entrances.len(), 3);
    assert_eq!(b.building(5).unwrap().entrances.len(), 3);
    assert!(b.building(4).unwrap().entrances.iter().all(|e| e[0] < 211.0));
    // Capacity and protection come from the class table.
    assert_eq!(td.squad_capacity(house.class, house.footprint_m2 as f64, house.height_m as f64), 2);
}

#[test]
fn vehicles_never_enter_buildings_and_squads_only_through_entrances() {
    let td = td();
    let b = map(&td);
    for (k, &id) in b.building_id.data.iter().enumerate() {
        if id != 0 {
            assert_eq!(b.mobility("tank").unwrap().speed.data[k], 0, "vehicles: buildings are no-go");
        }
    }
    let interior = terrain::mobility::quantise(td.foot.building_interior_speed_mps.get());
    assert_eq!(speed(&b, FOOT_CLASS, [106.0, 106.0]), interior, "interior cell");
    assert_eq!(speed(&b, FOOT_CLASS, [102.0, 102.0]), 0, "corner cell is wall");
    for e in &b.building(1).unwrap().entrances {
        // The cell just inside each entrance is open.
        let (cx, cy) = (106.0 + (e[0] as f64 - 106.0) * 0.7, 106.0 + (e[1] as f64 - 106.0) * 0.7);
        assert_eq!(speed(&b, FOOT_CLASS, [cx, cy]), interior, "entrance at {e:?}");
    }
    // Open ground: foot moves at the cross-country march rate.
    let cc = terrain::mobility::quantise(td.foot.cross_country_speed_mps.get());
    assert_eq!(speed(&b, FOOT_CLASS, [50.0, 50.0]), cc);
}

#[test]
fn a_foot_route_into_a_building_ends_inside_it() {
    let td = td();
    let b = map(&td);
    let foot = &b.mobility(FOOT_CLASS).unwrap().speed;
    let mut pf = Pathfinder::new();
    for id in [1, 2, 4] {
        let goal = b.building_goal(id).expect("enterable");
        let route = pf.find(foot, CELL, [20.0, 20.0], goal).expect("route");
        let end = *route.last().unwrap();
        assert_eq!(b.building_at(end[0], end[1]).map(|x| x.id), Some(id), "{route:?}");
        for w in route.windows(2) {
            assert!(segment_time(foot, CELL, w[0], w[1]).is_some(), "segment crosses a wall: {route:?}");
        }
    }
    assert!(b.building_goal(3).is_none(), "the shed cannot be entered");
    // A tank sent to the same point stops outside.
    let tank = &b.mobility("tank").unwrap().speed;
    let end = *pf.find(tank, CELL, [20.0, 20.0], b.building_goal(1).unwrap()).unwrap().last().unwrap();
    assert!(b.building_at(end[0], end[1]).is_none());
}

#[test]
fn foot_routes_go_around_buildings_rather_than_through_them() {
    let td = td();
    let b = map(&td);
    let foot = &b.mobility(FOOT_CLASS).unwrap().speed;
    let route = Pathfinder::new().find(foot, CELL, [90.0, 106.0], [125.0, 106.0]).unwrap();
    for w in route.windows(2) {
        for k in 0..=20 {
            let f = k as f64 / 20.0;
            let p = [w[0][0] + f * (w[1][0] - w[0][0]), w[0][1] + f * (w[1][1] - w[0][1])];
            assert!(b.building_at(p[0], p[1]).is_none(), "walked through a house at {p:?}: {route:?}");
        }
    }
}

#[test]
fn building_identity_survives_save_and_load() {
    let td = td();
    let b = map(&td);
    let dir = std::env::temp_dir().join(format!("tanks-test-buildings-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    b.save(&dir).unwrap();
    let back = BakedMap::load(&dir).unwrap();
    assert_eq!(back, b);
    let _ = std::fs::remove_dir_all(&dir);
}
