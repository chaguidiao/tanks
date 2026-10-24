#![allow(dead_code)]
//! Shared builders for synthetic test maps and scenarios.

use std::path::PathBuf;
use std::sync::Arc;

use sim::Sim;
use sim_data::{DataSet, MapRef, ScenarioDef, SideDef, UnitDef};
use terrain::mobility::MobilityClass;
use terrain::{AuthoringPackage, BakedMap, MapManifest, bake};

pub fn data() -> Arc<DataSet> {
    Arc::new(DataSet::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")).unwrap())
}

/// Flat open map (`w` × `h` cells of 4 m) with optional elevation function.
pub fn map(ds: &DataSet, w: usize, h: usize, cover: &str, z: impl Fn(f64, f64) -> f64) -> Arc<BakedMap> {
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
    p.landcover.data.fill(ds.terrain.landcover_id(cover).unwrap());
    for j in 0..h {
        for i in 0..w {
            let (x, y) = ((i as f64 + 0.5) * 4.0, (j as f64 + 0.5) * 4.0);
            p.elevation_base.set(i, j, (100.0 + z(x, y)) as f32);
        }
    }
    let classes: Vec<MobilityClass> = terrain::mobility::all_classes(ds);
    Arc::new(bake(&p, &ds.terrain, &classes))
}

/// Flat map like `map`, with the authoring package edited (buildings, forests) before baking.
pub fn map_with(
    ds: &DataSet,
    w: usize,
    h: usize,
    cover: &str,
    edit: impl FnOnce(&mut AuthoringPackage),
) -> Arc<BakedMap> {
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
    p.landcover.data.fill(ds.terrain.landcover_id(cover).unwrap());
    edit(&mut p);
    Arc::new(bake(&p, &ds.terrain, &terrain::mobility::all_classes(ds)))
}

/// A rectangular building footprint (counter-clockwise).
pub fn building(
    id: u32,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    class: sim_data::BuildingClass,
) -> terrain::features::Building {
    terrain::features::Building {
        id,
        ring: vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]],
        props: terrain::features::BuildingProps { height_m: 7.0, class },
    }
}

/// A squad on the ground, or mounted in `carrier` (listed before it).
pub fn squad(cs: &str, squad: &str, pos: [f64; 2], platoon: Option<&str>, carrier: Option<&str>) -> UnitDef {
    UnitDef {
        callsign: cs.into(),
        vehicle: String::new(),
        position: pos,
        heading_deg: 0.0,
        platoon: platoon.map(str::to_owned),
        squad: Some(squad.into()),
        mounted_in: carrier.map(str::to_owned),
    }
}

pub fn unit(cs: &str, vehicle: &str, pos: [f64; 2], heading_deg: f64, platoon: Option<&str>) -> UnitDef {
    UnitDef {
        callsign: cs.into(),
        vehicle: vehicle.into(),
        position: pos,
        heading_deg,
        platoon: platoon.map(str::to_owned),
        squad: None,
        mounted_in: None,
    }
}

pub fn scenario(start_time_s: u32, blue: Vec<UnitDef>, red: Vec<UnitDef>) -> ScenarioDef {
    ScenarioDef {
        id: "t".into(),
        name: "t".into(),
        date: (1990, 6, 1),
        start_time_s,
        map: MapRef { id: "synthetic".into(), recipe: "mixed-test".into(), seed: 1, size_m: 0.0 },
        seed: 7,
        sides: vec![
            SideDef {
                name: "BLUE".into(),
                doctrine: "western-combined".into(),
                tech_year: None,
                units: blue,
                ai: None,
                fire_support: vec![],
                supply_points: vec![],
            },
            SideDef {
                name: "RED".into(),
                doctrine: "generic-opfor".into(),
                tech_year: None,
                units: red,
                ai: None,
                fire_support: vec![],
                supply_points: vec![],
            },
        ],
        objectives: vec![],
        victory: None,
        weather: Default::default(),
        briefing: None,
        logistics: vec![],
    }
}

pub fn run(sim: &mut Sim, seconds: f64) -> Vec<sim::Event> {
    let mut ev = Vec::new();
    for _ in 0..(seconds * sim::TICK_HZ as f64) as usize {
        ev.extend(sim.step());
    }
    ev
}

pub fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}
