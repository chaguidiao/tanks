//! Procedural terrain generation (TERRAIN.md §3). Output is an authoring package, the same
//! format the editor and importers write. No graphics dependencies.
//!
//! Every stage is deterministic and seeded from its own stream (`stage_seed`). Randomness is
//! hash-based (position-keyed), so per-pixel work can run on threads without changing results.
//!
//! Implemented: 1 landform, 2 flow routing + stream-power/thermal erosion, 3 rivers,
//! 5 moisture/soil/land cover with field parcels, 6 villages + road network.
//! Not yet: lakes, stage 4 landform stamps, hedgerows, settlement interiors, stage 7 validation.

pub mod cover;
pub mod hydro;
pub mod landform;
pub mod recipe;
pub mod rivers;
pub mod roads;
pub mod workflow;

use sim_data::TerrainData;
use terrain::{AuthoringPackage, Grid, MapManifest};

pub use recipe::Recipe;

/// Independent seed per generation stage.
pub fn stage_seed(seed: u64, stage: u64) -> u64 {
    terrain::noise::hash_u64(seed ^ terrain::noise::hash_u64(stage.wrapping_mul(0x9e37_79b9_7f4a_7c15)))
}

/// Evaluate `f(i, j)` for every cell on worker threads. Each thread writes a disjoint band of
/// rows, so the result does not depend on scheduling.
pub fn par_grid<T: Copy + Default + Send>(w: usize, h: usize, f: impl Fn(usize, usize) -> T + Sync) -> Grid<T> {
    let mut data = vec![T::default(); w * h];
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(16);
    let rows_per = h.div_ceil(threads).max(1);
    std::thread::scope(|s| {
        for (band, chunk) in data.chunks_mut(rows_per * w).enumerate() {
            let f = &f;
            s.spawn(move || {
                for (k, v) in chunk.iter_mut().enumerate() {
                    let idx = band * rows_per * w + k;
                    *v = f(idx % w, idx / w);
                }
            });
        }
    });
    Grid::from_vec(w, h, data)
}

pub struct GenParams<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub seed: u64,
    pub size_m: f64,
    pub cell_m: f64,
}

/// Run all stages. `progress` receives a short stage label before each stage.
pub fn generate(
    recipe: &Recipe,
    recipe_ron: Option<String>,
    p: &GenParams,
    td: &TerrainData,
    mut progress: impl FnMut(&str),
) -> AuthoringPackage {
    let n = libm::round(p.size_m / p.cell_m) as usize;
    let manifest = MapManifest {
        id: p.id.to_owned(),
        name: p.name.to_owned(),
        width_cells: n,
        height_cells: n,
        cell_size_m: p.cell_m,
        seed: p.seed,
        recipe: Some(recipe.preset.clone()),
    };
    progress("landform");
    let mut hf = landform::generate(&recipe.landform, stage_seed(p.seed, 1), n, p.cell_m);
    progress("erosion");
    hydro::erode(&mut hf, &recipe.erosion);
    progress("hydrology");
    let flow = hydro::Flow::compute(&hf);
    progress("rivers");
    let mut features = terrain::Features { rivers: rivers::extract(&hf, &flow, &recipe.rivers), ..Default::default() };
    progress("cover");
    let cover = cover::generate(&hf, &flow, &features.rivers, &recipe.cover, td, stage_seed(p.seed, 5));
    progress("roads");
    let mut landcover = cover.landcover;
    let villages = roads::place_villages(&hf, &flow, &landcover, &recipe.roads, td, stage_seed(p.seed, 6));
    roads::stamp_villages(&mut landcover, &villages, p.cell_m, td, stage_seed(p.seed, 7));
    features.roads =
        roads::build_network(&hf, &landcover, &features.rivers, &villages, &recipe.roads, td, features.next_id());
    let mut pkg = AuthoringPackage::new_flat(manifest, 0.0);
    pkg.recipe_ron = recipe_ron;
    pkg.elevation_base = hf.z;
    pkg.landcover = landcover;
    pkg.soil = cover.soil;
    pkg.moisture = cover.moisture;
    pkg.features = features;
    pkg
}
