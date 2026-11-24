//! Bridges (M8d, TERRAIN.md §5.2): a road over a river is a bridge with a load class; a class
//! too heavy for it may not cross.

use std::path::PathBuf;

use sim_data::{DataSet, TerrainData};
use terrain::features::{Bridge, BridgeProps, River, RiverProps, Road, RoadClass, RoadProps};
use terrain::mobility::MobilityClass;
use terrain::{AuthoringPackage, BakedMap, MapManifest, bake};

fn td() -> TerrainData {
    DataSet::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")).unwrap().terrain
}

fn class(id: &str, mass_kg: f64) -> MobilityClass {
    MobilityClass {
        id: id.into(),
        road_speed: 18.0,
        cross_country_speed: 12.0,
        max_slope: 0.5,
        fording_depth: 1.2,
        foot: false,
        mass_kg,
    }
}

/// 400 m square: a river west to east at y 200, two roads crossing it north to south at x 100
/// and x 300; `authored` bridge classes on the west road.
fn map(td: &TerrainData, authored: Option<f32>) -> BakedMap {
    let m = MapManifest {
        id: "b".into(),
        name: "b".into(),
        width_cells: 100,
        height_cells: 100,
        cell_size_m: 4.0,
        seed: 1,
        recipe: None,
    };
    let mut p = AuthoringPackage::new_flat(m, 100.0);
    p.landcover.data.fill(td.landcover_id("grassland").unwrap());
    p.features.rivers.push(River {
        id: 1,
        points: vec![[0.0, 200.0], [400.0, 200.0]],
        props: RiverProps { width_m: vec![20.0, 20.0], depth_m: vec![3.0, 3.0] },
    });
    for (id, x) in [(2, 100.0), (3, 300.0)] {
        p.features.roads.push(Road {
            id,
            points: vec![[x, 0.0], [x, 400.0]],
            props: RoadProps { class: RoadClass::Secondary, width_m: 6.0 },
        });
    }
    if let Some(mlc) = authored {
        p.features.bridges.push(Bridge {
            id: 4,
            points: vec![[100.0, 180.0], [100.0, 220.0]],
            props: BridgeProps { mlc },
        });
    }
    bake(&p, td, &[class("light", 20_000.0), class("heavy", 60_000.0)])
}

#[test]
fn roads_over_water_are_bridges_with_the_default_class() {
    let td = td();
    let b = map(&td, None);
    assert_eq!(b.bridges.len(), 2, "{:?}", b.bridges.iter().map(|x| x.centre).collect::<Vec<_>>());
    assert!(b.bridges.iter().all(|x| x.mlc == td.bridges.default_mlc.get() as f32 && !x.authored));
    let xs: Vec<f32> = b.bridges.iter().map(|x| x.centre[0]).collect();
    assert!((xs[0] - 100.0).abs() < 8.0 && (xs[1] - 300.0).abs() < 8.0, "{xs:?}");
    // Both classes cross both bridges at the default class.
    for id in ["light", "heavy"] {
        let s = &b.mobility(id).unwrap().speed;
        assert!(b.bridges.iter().all(|x| x.cells.iter().all(|&k| s.data[k as usize] > 0)), "{id}");
    }
}

#[test]
fn an_authored_class_closes_the_bridge_to_heavier_vehicles() {
    // 60 t is about 66 short tons; a class 40 bridge carries the 20 t vehicle only.
    let td = td();
    let b = map(&td, Some(40.0));
    let west = b.bridges.iter().find(|x| (x.centre[0] - 100.0).abs() < 8.0).unwrap();
    assert!(west.authored && west.mlc == 40.0);
    let heavy = &b.mobility("heavy").unwrap().speed;
    let light = &b.mobility("light").unwrap().speed;
    assert!(west.cells.iter().all(|&k| heavy.data[k as usize] == 0));
    assert!(west.cells.iter().all(|&k| light.data[k as usize] > 0));
    // Saved and loaded: the same bridges.
    let dir = std::env::temp_dir().join(format!("tanks-bridges-{}", std::process::id()));
    b.save(&dir).unwrap();
    let back = BakedMap::load(&dir).unwrap();
    assert_eq!(back.bridges, b.bridges);
}
