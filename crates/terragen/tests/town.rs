//! M8h1b towns (plan `docs/plans/M8H1B-TOWNS.md` §5): the `market-town` recipe makes a town with
//! streets, closed blocks and every zone; courtyards are shut to vehicles but reachable on foot;
//! recipes without a town generate exactly as before.

use std::collections::VecDeque;
use std::path::PathBuf;

use sim_data::{BuildingClass, DataSet};
use terragen::{GenParams, workflow};
use terrain::features::RoadClass;
use terrain::{AuthoringPackage, Grid, bake};

fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")
}

fn generate(recipe: &str, seed: u64, size_m: f64) -> (DataSet, AuthoringPackage) {
    let data = data_dir();
    let ds = DataSet::load(&data).unwrap();
    let (r, text) = workflow::load_recipe(&data, recipe).unwrap();
    let p = GenParams { id: "t", name: "t", seed, size_m, cell_m: 4.0 };
    let pkg = terragen::generate(&r, Some(text), &p, &ds.terrain, |_| {});
    (ds, pkg)
}

#[test]
fn market_town_has_streets_blocks_and_every_zone() {
    let (ds, a) = generate("market-town", 42, 3200.0);
    let (_, b) = generate("market-town", 42, 3200.0);
    assert_eq!(a.input_hash(), b.input_hash(), "deterministic");
    let streets = a.features.roads.iter().filter(|r| r.props.class == RoadClass::Street).count();
    assert!(streets >= 30, "{streets} streets");
    let count = |c: BuildingClass| a.features.buildings.iter().filter(|b| b.props.class == c).count();
    let (masonry, concrete, industrial) =
        (count(BuildingClass::Masonry), count(BuildingClass::Concrete), count(BuildingClass::Industrial));
    println!("streets {streets}, masonry {masonry}, concrete {concrete}, industrial {industrial}");
    assert!(masonry >= 500, "{masonry} masonry (core rows and houses)");
    assert!(concrete >= 6, "{concrete} slab blocks");
    assert!(industrial >= 6, "{industrial} sheds");
    let town = ds.terrain.landcover_id("town").unwrap();
    let town_cells = a.landcover.data.iter().filter(|&&v| v == town).count();
    assert!(town_cells as f64 * 16.0 > 600_000.0, "town ground {:.2} km²", town_cells as f64 * 16.0 / 1e6);
    // Routed roads reach the streets: some routed road ends on a street node.
    let ends: Vec<_> = a
        .features
        .roads
        .iter()
        .filter(|r| r.props.class != RoadClass::Street)
        .flat_map(|r| [r.points[0], *r.points.last().unwrap()])
        .collect();
    let on_street = ends
        .iter()
        .any(|e| a.features.roads.iter().filter(|r| r.props.class == RoadClass::Street).any(|s| s.points.contains(e)));
    assert!(on_street, "a routed road joins the streets");
}

/// Component labels of `open` cells inside `keep` (8-connected), largest first by size.
fn components(open: &Grid<bool>, keep: &Grid<bool>) -> (Grid<u32>, Vec<usize>) {
    let (w, h) = (open.w, open.h);
    let mut lab = Grid::new(w, h, 0u32);
    let mut sizes = vec![0usize];
    for j in 0..h {
        for i in 0..w {
            if !open.get(i, j) || !keep.get(i, j) || lab.get(i, j) != 0 {
                continue;
            }
            let id = sizes.len() as u32;
            let mut n = 0;
            let mut q = VecDeque::from([(i, j)]);
            lab.set(i, j, id);
            while let Some((x, y)) = q.pop_front() {
                n += 1;
                for dy in -1i64..=1 {
                    for dx in -1i64..=1 {
                        let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                        if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                            continue;
                        }
                        let (nx, ny) = (nx as usize, ny as usize);
                        if open.get(nx, ny) && keep.get(nx, ny) && lab.get(nx, ny) == 0 {
                            lab.set(nx, ny, id);
                            q.push_back((nx, ny));
                        }
                    }
                }
            }
            sizes.push(n);
        }
    }
    (lab, sizes)
}

#[test]
fn courtyards_are_shut_to_vehicles_and_open_on_foot() {
    let (ds, pkg) = generate("market-town", 42, 3200.0);
    let baked = bake(&pkg, &ds.terrain, &workflow::mobility_classes(&ds));
    let (w, h) = (baked.landcover.w, baked.landcover.h);
    let town = ds.terrain.landcover_id("town").unwrap();
    let road = ds.terrain.landcover_id("road").unwrap();
    // The town: town ground, its streets, and its buildings.
    let mut keep = Grid::new(w, h, false);
    for k in 0..w * h {
        let lc = baked.landcover.data[k];
        keep.data[k] = lc == town || (lc == road && baked.building_id.data[k] == 0) || baked.building_id.data[k] != 0;
    }
    let tank = baked.mobility("bulwark_a1").unwrap();
    let foot = baked.mobility("foot").unwrap();
    let veh = Grid::from_vec(w, h, tank.speed.data.iter().map(|&v| v > 0).collect());
    let (lab, sizes) = components(&veh, &keep);
    let streets = (1..sizes.len()).max_by_key(|&k| sizes[k]).unwrap() as u32;
    // Courtyards: closed pockets of open ground of at least 400 m² (25 cells).
    let yards: Vec<u32> = (1..sizes.len() as u32).filter(|&k| k != streets && sizes[k as usize] >= 25).collect();
    assert!(yards.len() >= 15, "{} closed courtyards", yards.len());
    // On foot (4-connected, through the houses' entrances) from the streets.
    let start = lab.data.iter().position(|&l| l == streets).unwrap();
    let mut seen = vec![false; w * h];
    seen[start] = true;
    let mut q = VecDeque::from([start]);
    while let Some(k) = q.pop_front() {
        let (i, j) = (k % w, k / w);
        for (di, dj) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
            let (ni, nj) = (i as i64 + di, j as i64 + dj);
            if ni < 0 || nj < 0 || ni >= w as i64 || nj >= h as i64 {
                continue;
            }
            let nk = nj as usize * w + ni as usize;
            if !seen[nk] && keep.data[nk] && foot.speed.data[nk] > 0 {
                seen[nk] = true;
                q.push_back(nk);
            }
        }
    }
    let reached = yards.iter().filter(|&&y| (0..w * h).any(|k| lab.data[k] == y && seen[k])).count();
    println!("courtyards {}, reached on foot {reached}", yards.len());
    assert!(reached * 10 >= yards.len() * 9, "{reached} of {} courtyards reachable on foot", yards.len());
}

#[test]
fn recipes_without_a_town_generate_as_before() {
    // Pinned 2026-10-29 with the town code in place; the full `demo` and `uplands` maps were
    // checked byte-identical (authoring and baked hashes) against the build before M8h1b.
    let (_, pkg) = generate("rolling-farmland", 3, 1600.0);
    assert_eq!(pkg.input_hash(), 6800451673710119301, "rolling-farmland 1600 m seed 3");
}
