//! Load-or-generate helpers shared by the CLI and the client.

use std::path::{Path, PathBuf};

use sim_data::{DataSet, MapRef};
use terrain::bake::BAKER_VERSION;
use terrain::mobility::MobilityClass;
use terrain::{AuthoringPackage, BakedMap, Result, TerrainError, bake};

use crate::{GenParams, Recipe};

pub fn map_dir(data: &Path, id: &str) -> PathBuf {
    data.join("maps").join(id)
}

pub fn load_recipe(data: &Path, preset: &str) -> Result<(Recipe, String)> {
    let p = data.join("recipes").join(format!("{preset}.ron"));
    let text = std::fs::read_to_string(&p).map_err(|e| TerrainError::io(&p, e))?;
    let r = Recipe::from_ron(&text).map_err(|e| TerrainError::io(&p, e))?;
    Ok((r, text))
}

pub fn mobility_classes(ds: &DataSet) -> Vec<MobilityClass> {
    ds.vehicles.iter().map(MobilityClass::from).collect()
}

/// Generate an authoring package and write it to `data/maps/<id>`.
pub fn generate_map(
    data: &Path,
    ds: &DataSet,
    preset: &str,
    id: &str,
    seed: u64,
    size_m: f64,
    mut log: impl FnMut(&str),
) -> Result<AuthoringPackage> {
    let (recipe, text) = load_recipe(data, preset)?;
    let p = GenParams { id, name: id, seed, size_m, cell_m: 4.0 };
    let pkg = crate::generate(&recipe, Some(text), &p, &ds.terrain, |s| log(&format!("gen: {s}")));
    log("gen: writing authoring package");
    pkg.save(&map_dir(data, id))?;
    Ok(pkg)
}

/// Bake an authoring package to `data/maps/<id>/baked`.
pub fn bake_map(data: &Path, ds: &DataSet, pkg: &AuthoringPackage) -> Result<BakedMap> {
    let baked = bake(pkg, &ds.terrain, &mobility_classes(ds));
    baked.save(&map_dir(data, &pkg.manifest.id).join("baked"))?;
    Ok(baked)
}

/// Load the baked map, generating and/or (re)baking as needed.
pub fn ensure_map(data: &Path, ds: &DataSet, map: &MapRef, mut log: impl FnMut(&str)) -> Result<BakedMap> {
    let dir = map_dir(data, &map.id);
    let pkg = if dir.join("map.toml").exists() {
        AuthoringPackage::load(&dir)?
    } else {
        log(&format!("map '{}' not found; generating from recipe '{}' seed {}", map.id, map.recipe, map.seed));
        generate_map(data, ds, &map.recipe, &map.id, map.seed, map.size_m, &mut log)?
    };
    match BakedMap::load(&dir.join("baked")) {
        Ok(b) if b.input_hash == pkg.input_hash() && classes_match(&b, ds) => Ok(b),
        _ => {
            log(&format!("baking '{}' (baker v{BAKER_VERSION})", map.id));
            bake_map(data, ds, &pkg)
        }
    }
}

fn classes_match(b: &BakedMap, ds: &DataSet) -> bool {
    let want = mobility_classes(ds);
    want.len() == b.mobility.len() && want.iter().zip(&b.mobility).all(|(a, m)| *a == m.class)
}
