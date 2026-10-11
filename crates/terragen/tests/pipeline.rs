//! End-to-end: generate → save/load authoring → bake → save/load baked, on a small map.

use std::path::PathBuf;

use sim_data::DataSet;
use terragen::{GenParams, workflow};
use terrain::{AuthoringPackage, BakedMap, bake};

fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")
}

fn small(seed: u64) -> (DataSet, AuthoringPackage) {
    let data = data_dir();
    let ds = DataSet::load(&data).unwrap();
    let (recipe, text) = workflow::load_recipe(&data, "mixed-test").unwrap();
    let p = GenParams { id: "t", name: "t", seed, size_m: 1600.0, cell_m: 4.0 };
    let pkg = terragen::generate(&recipe, Some(text), &p, &ds.terrain, |_| {});
    (ds, pkg)
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("tanks-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

#[test]
fn generation_is_deterministic_and_seed_dependent() {
    let (_, a) = small(7);
    let (_, b) = small(7);
    let (_, c) = small(8);
    assert_eq!(a.input_hash(), b.input_hash());
    assert_ne!(a.input_hash(), c.input_hash());
}

#[test]
fn authoring_package_round_trips() {
    let (_, pkg) = small(3);
    let dir = scratch("authoring");
    pkg.save(&dir).unwrap();
    let back = AuthoringPackage::load(&dir).unwrap();
    assert_eq!(back.manifest, pkg.manifest);
    assert!(back.elevation_base == pkg.elevation_base, "elevation_base");
    assert!(back.edit_height == pkg.edit_height, "edit_height");
    assert!(back.landcover == pkg.landcover, "landcover");
    assert!(back.soil == pkg.soil, "soil");
    assert!(back.moisture == pkg.moisture, "moisture");
    assert_eq!(back.features.rivers, pkg.features.rivers);
    assert_eq!(back.features.roads, pkg.features.roads);
    assert_eq!(back.input_hash(), pkg.input_hash());
    assert_eq!(back, pkg);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bake_is_deterministic_and_round_trips() {
    let (ds, pkg) = small(5);
    let classes = workflow::mobility_classes(&ds);
    let b1 = bake(&pkg, &ds.terrain, &classes);
    let b2 = bake(&pkg, &ds.terrain, &classes);
    assert_eq!(b1.output_hash(), b2.output_hash());
    let dir = scratch("baked");
    b1.save(&dir).unwrap();
    let back = BakedMap::load(&dir).unwrap();
    assert_eq!(back.output_hash(), b1.output_hash());
    assert_eq!(back.elevation, b1.elevation);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rivers_flow_downhill_and_carve_water() {
    let (ds, pkg) = small(11);
    assert!(!pkg.features.rivers.is_empty(), "test map should have rivers");
    let baked = bake(&pkg, &ds.terrain, &[]);
    for r in &pkg.features.rivers {
        // Ends on the map edge or on another river (junction).
        let end = *r.points.last().unwrap();
        let size = pkg.manifest.width_m();
        let on_edge = end[0] < 8.0 || end[1] < 8.0 || end[0] > size - 8.0 || end[1] > size - 8.0;
        let joins = pkg.features.rivers.iter().any(|o| {
            o.id != r.id && o.points.iter().any(|p| (p[0] - end[0]).abs() < 6.0 && (p[1] - end[1]).abs() < 6.0)
        });
        assert!(on_edge || joins, "river {} ends inland at {end:?}", r.id);
    }
    let wet = baked.water_depth.data.iter().filter(|&&d| d > 0.2).count();
    assert!(wet > 50, "carved channels hold water ({wet} cells)");
}
