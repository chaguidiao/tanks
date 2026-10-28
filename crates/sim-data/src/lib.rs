//! Data definitions and loaders: vehicles, terrain classes, scenarios.
//!
//! Every numeric stat is a [`Stat`] carrying its source and confidence (CLAUDE.md data rules).
//! Units are SI throughout (m, s, kg, W, rad).

pub mod combat;
pub mod doctrine;
pub mod fires;
pub mod infantry;
pub mod logistics;
pub mod scenario;
pub mod smoke;
pub mod stat;
pub mod terrain;
pub mod vehicle;

use std::path::{Path, PathBuf};

pub use combat::*;
pub use doctrine::*;
pub use fires::*;
pub use infantry::*;
pub use logistics::*;
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
    pub squads: Vec<SquadDef>,
    pub ammo: Vec<AmmoDef>,
    pub doctrine: Vec<DoctrinePack>,
    pub detection: DetectionModel,
    pub damage: DamageTables,
    pub penetration: PenetrationModel,
    pub smoke: SmokeModel,
    pub weather: WeatherTable,
    pub fires: FireSupportModel,
    pub logistics: LogisticsModel,
    pub infantry: InfantryModel,
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
        let mut squads = Vec::new();
        for p in ron_files(&root.join("infantry"))? {
            squads.push(load_ron::<SquadDef>(&p)?);
        }
        squads.sort_by(|a, b| a.id.cmp(&b.id));
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
            buildings: load_ron(&root.join("terrain/buildings.ron"))?,
            foot: load_ron(&root.join("terrain/foot.ron"))?,
        };
        terrain.validate().map_err(|message| DataError { path: root.join("terrain"), message })?;
        let ds = Self {
            vehicles,
            squads,
            ammo,
            doctrine,
            detection: load_ron(&root.join("combat/detection.ron"))?,
            damage: load_ron(&root.join("combat/damage.ron"))?,
            penetration: load_ron(&root.join("combat/penetration.ron"))?,
            smoke: load_ron(&root.join("combat/smoke.ron"))?,
            weather: load_ron(&root.join("combat/weather.ron"))?,
            fires: load_ron(&root.join("combat/fires.ron"))?,
            logistics: load_ron(&root.join("combat/logistics.ron"))?,
            infantry: load_ron(&root.join("combat/infantry.ron"))?,
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
        for s in &ds.squads {
            if ds.vehicle(&s.id).is_some() || s.men == 0 || s.weapons.is_empty() {
                return Err(DataError {
                    path: root.join("infantry"),
                    message: format!("{}: squad ids must differ from vehicle ids; needs men and weapons", s.id),
                });
            }
            for w in &s.anti_armour {
                let bad = w.load.is_empty() || w.count == 0 || w.load.iter().any(|l| ds.ammo(&l.ammo).is_none());
                if bad {
                    return Err(DataError {
                        path: root.join("infantry"),
                        message: format!("{}: {} needs launchers and known rounds", s.id, w.name),
                    });
                }
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

    pub fn squad(&self, id: &str) -> Option<&SquadDef> {
        self.squads.iter().find(|s| s.id == id)
    }
}

/// Locate the repository `data/` directory: `$TANKS_DATA`, else walk up from the current dir.
/// Locate the `data/` directory: `$TANKS_DATA`, else search upward from the current directory
/// (development: run from anywhere in the repository), else upward from the executable's folder
/// (a packaged release launched from a shortcut or another directory).
pub fn find_data_dir() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("TANKS_DATA") {
        return Some(PathBuf::from(p));
    }
    let cwd = std::env::current_dir().ok();
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
    cwd.into_iter().chain(exe_dir).find_map(|start| find_data_from(&start))
}

/// The nearest `data/` directory (one holding `vehicles/`) in `start` or its ancestors.
pub fn find_data_from(start: &Path) -> Option<PathBuf> {
    start.ancestors().map(|d| d.join("data")).find(|c| c.join("vehicles").is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_is_found_from_a_nested_folder_and_not_from_an_empty_one() {
        let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let found = find_data_from(&here.join("src")).expect("data/ above the crate");
        assert!(found.join("vehicles").is_dir());
        let empty = std::env::temp_dir().join("tanks-no-data-here");
        std::fs::create_dir_all(&empty).unwrap();
        // Unless a parent of the temp dir happens to hold a data/ folder with vehicles/.
        assert!(find_data_from(&empty).is_none_or(|d| d.join("vehicles").is_dir()));
    }

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

    #[test]
    fn supply_points_round_trip_and_default_to_empty() {
        let text = r#"(name: "BLUE", doctrine: "western-combined", units: [],
            supply_points: [(callsign: "SP 1", position: (100.0, 200.0), fuel_l: 4000.0,
                rounds: [("apfsds_120a", 80)], smoke_salvos: 8)])"#;
        let side: SideDef = ron_options().from_str(text).unwrap();
        let sp = &side.supply_points[0];
        assert_eq!((sp.callsign.as_str(), sp.fuel_l, sp.smoke_salvos, sp.unlimited), ("SP 1", 4000.0, 8, false));
        assert_eq!(sp.rounds, vec![("apfsds_120a".to_owned(), 80)]);
        let back: SideDef = ron_options().from_str(&ron_options().to_string(&side).unwrap()).unwrap();
        assert_eq!(back.supply_points[0].rounds, sp.rounds);
        let none: SideDef = ron_options().from_str(r#"(name: "RED", doctrine: "generic-opfor", units: [])"#).unwrap();
        assert!(none.supply_points.is_empty());
        let ser = ron_options().to_string(&none).unwrap();
        assert!(!ser.contains("supply_points"), "omitted when empty, so old files stay unchanged");
    }

    #[test]
    fn logistics_events_round_trip_and_default_to_empty() {
        let text = r#"[(at_s: 1800.0, side: 0, platoons: ["1"], kind: Rearm(fraction: 0.5)),
            (at_s: 2400.0, side: 1, kind: Battery(callsign: "HAMMER", he: 60))]"#;
        let ev: Vec<LogisticsEventDef> = ron_options().from_str(text).unwrap();
        assert_eq!(ev[0].kind, LogisticsKind::Rearm { fraction: 0.5 });
        assert!(ev[1].platoons.is_empty(), "empty = every platoon");
        assert_eq!(ev[1].kind, LogisticsKind::Battery { callsign: "HAMMER".into(), he: 60, smoke: 0 });
        let back: Vec<LogisticsEventDef> = ron_options().from_str(&ron_options().to_string(&ev).unwrap()).unwrap();
        assert_eq!(back, ev);
    }

    #[test]
    fn stow_classes_follow_the_data() {
        let ds = DataSet::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")).unwrap();
        let class = |id: &str| ds.ammo(id).unwrap().stow_class();
        assert_eq!(class("apfsds_120a"), StowClass::MainGun);
        assert_eq!(class("apds_25"), StowClass::Autocannon);
        assert_eq!(class("atgm_tow"), StowClass::Missile);
        assert_eq!(class("atgm_refleks"), StowClass::MainGun, "gun-launched");
        assert!(ds.logistics.stations.get() >= 1.0);
    }

    /// The scenario list shows names (and the year), so two scenarios with one name are
    /// indistinguishable to the player.
    #[test]
    fn scenario_names_are_unique() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let mut seen: std::collections::BTreeMap<String, String> = Default::default();
        for id in DataSet::scenario_ids(&root) {
            let name = DataSet::load_scenario(&root, &id).unwrap().name;
            if let Some(other) = seen.insert(name.clone(), id.clone()) {
                panic!("scenarios {other} and {id} are both named {name:?}");
            }
        }
    }
}
