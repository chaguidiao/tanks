//! Data definitions and loaders: vehicles, terrain classes, scenarios.
//!
//! Every numeric stat is a [`Stat`] carrying its source and confidence (CLAUDE.md data rules).
//! Units are SI throughout (m, s, kg, W, rad).

pub mod combat;
pub mod doctrine;
pub mod scenario;
pub mod smoke;
pub mod stat;
pub mod terrain;
pub mod vehicle;

use std::path::{Path, PathBuf};

pub use combat::*;
pub use doctrine::*;
pub use scenario::*;
pub use smoke::*;
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
    pub ammo: Vec<AmmoDef>,
    pub doctrine: Vec<DoctrinePack>,
    pub detection: DetectionModel,
    pub damage: DamageTables,
    pub penetration: PenetrationModel,
    pub smoke: SmokeModel,
    pub weather: WeatherTable,
    pub terrain: TerrainData,
}

fn ron_files(dir: &Path) -> Result<Vec<PathBuf>, DataError> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| DataError { path: dir.to_owned(), message: e.to_string() })?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "ron"))
        .collect();
    paths.sort();
    Ok(paths)
}

impl DataSet {
    pub fn load(root: &Path) -> Result<Self, DataError> {
        let mut vehicles = Vec::new();
        for p in ron_files(&root.join("vehicles"))? {
            vehicles.push(load_ron::<VehicleDef>(&p)?);
        }
        vehicles.sort_by(|a, b| a.id.cmp(&b.id));
        let mut ammo = Vec::new();
        for p in ron_files(&root.join("ammo"))? {
            ammo.push(load_ron::<AmmoDef>(&p)?);
        }
        ammo.sort_by(|a, b| a.id.cmp(&b.id));
        let mut doctrine = Vec::new();
        let ddir = root.join("doctrine");
        let mut packs: Vec<PathBuf> = std::fs::read_dir(&ddir)
            .map_err(|e| DataError { path: ddir.clone(), message: e.to_string() })?
            .filter_map(|e| e.ok().map(|e| e.path().join("doctrine.ron")))
            .filter(|p| p.exists())
            .collect();
        packs.sort();
        for p in packs {
            doctrine.push(load_ron::<DoctrinePack>(&p)?);
        }
        let terrain = TerrainData {
            landcover: load_ron(&root.join("terrain/landcover.ron"))?,
            soils: load_ron(&root.join("terrain/soils.ron"))?,
        };
        terrain.validate().map_err(|message| DataError { path: root.join("terrain"), message })?;
        let ds = Self {
            vehicles,
            ammo,
            doctrine,
            detection: load_ron(&root.join("combat/detection.ron"))?,
            damage: load_ron(&root.join("combat/damage.ron"))?,
            penetration: load_ron(&root.join("combat/penetration.ron"))?,
            smoke: load_ron(&root.join("combat/smoke.ron"))?,
            weather: load_ron(&root.join("combat/weather.ron"))?,
            terrain,
        };
        for a in &ds.ammo {
            let bad = |what: &str| DataError { path: root.join("ammo"), message: format!("{}: {what}", a.id) };
            match a.kind {
                AmmoKind::Apfsds if a.rod().is_none() => return Err(bad("APFSDS needs rod_length/diameter/density")),
                AmmoKind::Apds | AmmoKind::Heat | AmmoKind::HeFrag if a.flat_penetration_mm.is_none() => {
                    return Err(bad("needs flat_penetration_mm"));
                }
                AmmoKind::Smoke => match &a.smoke {
                    Some(s) if ds.smoke.kind(&s.kind).is_some() => {}
                    _ => return Err(bad("smoke round needs a known smoke kind")),
                },
                _ => {}
            }
        }
        for v in &ds.vehicles {
            for l in &v.armament.load {
                if ds.ammo(&l.ammo).is_none() {
                    return Err(DataError {
                        path: root.join("vehicles"),
                        message: format!("{}: unknown ammo '{}'", v.id, l.ammo),
                    });
                }
            }
            let kinds =
                v.smoke_launcher.iter().flat_map(|l| &l.kinds).chain(v.engine_smoke.iter().flat_map(|e| &e.kinds));
            for k in kinds {
                if ds.smoke.kind(k).is_none() {
                    return Err(DataError {
                        path: root.join("vehicles"),
                        message: format!("{}: unknown smoke kind '{k}'", v.id),
                    });
                }
            }
        }
        Ok(ds)
    }

    pub fn ammo(&self, id: &str) -> Option<&AmmoDef> {
        self.ammo.iter().find(|a| a.id == id)
    }

    pub fn doctrine_pack(&self, id: &str) -> Option<&DoctrinePack> {
        self.doctrine.iter().find(|d| d.id == id)
    }

    pub fn load_scenario(root: &Path, id: &str) -> Result<ScenarioDef, DataError> {
        load_ron(&root.join("scenarios").join(format!("{id}.ron")))
    }

    /// Ids of every scenario in `data/scenarios/`, sorted.
    pub fn scenario_ids(root: &Path) -> Vec<String> {
        let mut ids: Vec<String> = std::fs::read_dir(root.join("scenarios"))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                (p.extension()? == "ron").then(|| p.file_stem()?.to_str().map(str::to_owned))?
            })
            .collect();
        ids.sort();
        ids
    }

    /// Write a scenario to `data/scenarios/<id>.ron` (scenario editor). Returns the path.
    pub fn save_scenario(root: &Path, def: &ScenarioDef) -> Result<PathBuf, DataError> {
        let path = root.join("scenarios").join(format!("{}.ron", def.id));
        let err = |message: String| DataError { path: path.clone(), message };
        let text = ron_options()
            .to_string_pretty(def, ron::ser::PrettyConfig::default().struct_names(false))
            .map_err(|e| err(e.to_string()))?;
        std::fs::write(&path, text).map_err(|e| err(e.to_string()))?;
        Ok(path)
    }

    /// Vehicles in service in `year` (era is data).
    pub fn vehicles_in_service(&self, year: i32) -> impl Iterator<Item = &VehicleDef> {
        self.vehicles.iter().filter(move |v| v.in_service.contains(year))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_scenario_loads() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let ids = DataSet::scenario_ids(&root);
        assert!(ids.contains(&"m1-move".to_owned()) && ids.contains(&"m5-hold-the-village".to_owned()));
        assert!(ids.windows(2).all(|w| w[0] < w[1]));
        for id in &ids {
            let s = DataSet::load_scenario(&root, id).unwrap_or_else(|e| panic!("{id}: {e}"));
            assert_eq!(&s.id, id);
        }
    }
}
