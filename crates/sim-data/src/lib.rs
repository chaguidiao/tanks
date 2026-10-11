//! Data definitions and loaders: vehicles, terrain classes, scenarios.
//!
//! Every numeric stat is a [`Stat`] carrying its source and confidence (CLAUDE.md data rules).
//! Units are SI throughout (m, s, kg, W, rad).

pub mod scenario;
pub mod stat;
pub mod terrain;
pub mod vehicle;

use std::path::{Path, PathBuf};

pub use scenario::*;
pub use stat::*;
pub use terrain::*;
pub use vehicle::*;

#[derive(Debug)]
pub struct DataError {
    pub path: PathBuf,
    pub message: String,
}

impl std::fmt::Display for DataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.message)
    }
}

impl std::error::Error for DataError {}

pub fn load_ron<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, DataError> {
    let text =
        std::fs::read_to_string(path).map_err(|e| DataError { path: path.to_owned(), message: e.to_string() })?;
    ron_options().from_str(&text).map_err(|e| DataError { path: path.to_owned(), message: e.to_string() })
}

/// RON options for all data files: optional fields may be written without `Some(..)`.
pub fn ron_options() -> ron::Options {
    ron::Options::default().with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME)
}

/// Everything loaded from `data/`. Collections are sorted by id so iteration is deterministic.
#[derive(Debug, Clone)]
pub struct DataSet {
    pub vehicles: Vec<VehicleDef>,
    pub terrain: TerrainData,
}

impl DataSet {
    pub fn load(root: &Path) -> Result<Self, DataError> {
        let mut vehicles = Vec::new();
        let vdir = root.join("vehicles");
        let mut paths: Vec<PathBuf> = std::fs::read_dir(&vdir)
            .map_err(|e| DataError { path: vdir.clone(), message: e.to_string() })?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "ron"))
            .collect();
        paths.sort();
        for p in paths {
            vehicles.push(load_ron::<VehicleDef>(&p)?);
        }
        vehicles.sort_by(|a, b| a.id.cmp(&b.id));
        let terrain = TerrainData {
            landcover: load_ron(&root.join("terrain/landcover.ron"))?,
            soils: load_ron(&root.join("terrain/soils.ron"))?,
        };
        terrain.validate().map_err(|message| DataError { path: root.join("terrain"), message })?;
        Ok(Self { vehicles, terrain })
    }

    pub fn load_scenario(root: &Path, id: &str) -> Result<ScenarioDef, DataError> {
        load_ron(&root.join("scenarios").join(format!("{id}.ron")))
    }

    pub fn vehicle(&self, id: &str) -> Option<&VehicleDef> {
        self.vehicles.iter().find(|v| v.id == id)
    }
}

/// Locate the repository `data/` directory: `$TANKS_DATA`, else walk up from the current dir.
pub fn find_data_dir() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("TANKS_DATA") {
        return Some(PathBuf::from(p));
    }
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let cand = dir.join("data");
        if cand.join("vehicles").is_dir() {
            return Some(cand);
        }
        if !dir.pop() {
            return None;
        }
    }
}
