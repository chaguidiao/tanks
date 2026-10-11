//! Vector features of the authoring package, stored as GeoJSON FeatureCollections in local
//! metric coordinates (TERRAIN.md §2.2). Each feature has a stable numeric id.

use std::path::Path;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{Result, TerrainError, Vec2};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Features {
    pub rivers: Vec<River>,
    pub roads: Vec<Road>,
    pub forests: Vec<Forest>,
    pub buildings: Vec<Building>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildingClass {
    Wood,
    Masonry,
    Concrete,
    Industrial,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildingProps {
    pub height_m: f32,
    pub class: BuildingClass,
}

/// Building footprint (TERRAIN.md §5.4).
#[derive(Debug, Clone, PartialEq)]
pub struct Building {
    pub id: u32,
    pub ring: Vec<Vec2>,
    pub props: BuildingProps,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiverProps {
    /// Per-vertex channel width (m).
    pub width_m: Vec<f32>,
    /// Per-vertex water depth (m).
    pub depth_m: Vec<f32>,
}

/// River centreline, ordered upstream to downstream.
#[derive(Debug, Clone, PartialEq)]
pub struct River {
    pub id: u32,
    pub points: Vec<Vec2>,
    pub props: RiverProps,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoadClass {
    Main,
    Secondary,
    Track,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoadProps {
    pub class: RoadClass,
    pub width_m: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Road {
    pub id: u32,
    pub points: Vec<Vec2>,
    pub props: RoadProps,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForestKind {
    Deciduous,
    Coniferous,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForestProps {
    pub kind: ForestKind,
    /// Mean canopy height (m).
    pub height_m: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Forest {
    pub id: u32,
    /// Outer ring (not repeated at the end).
    pub ring: Vec<Vec2>,
    pub props: ForestProps,
}

// --- minimal GeoJSON encoding -------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct Collection<P> {
    #[serde(rename = "type")]
    ty: String,
    features: Vec<Feature<P>>,
}

#[derive(Serialize, Deserialize)]
struct Feature<P> {
    #[serde(rename = "type")]
    ty: String,
    id: u32,
    geometry: Geometry,
    properties: P,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum Geometry {
    LineString { coordinates: Vec<Vec2> },
    Polygon { coordinates: Vec<Vec<Vec2>> },
}

fn write_collection<P: Serialize>(path: &Path, items: Vec<(u32, Geometry, P)>) -> Result<()> {
    let c = Collection {
        ty: "FeatureCollection".into(),
        features: items
            .into_iter()
            .map(|(id, geometry, properties)| Feature { ty: "Feature".into(), id, geometry, properties })
            .collect(),
    };
    let text = serde_json::to_string_pretty(&c).map_err(|e| TerrainError::io(path, e))?;
    std::fs::write(path, text).map_err(|e| TerrainError::io(path, e))
}

fn read_collection<P: DeserializeOwned>(path: &Path) -> Result<Vec<(u32, Geometry, P)>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(path).map_err(|e| TerrainError::io(path, e))?;
    let c: Collection<P> = serde_json::from_str(&text).map_err(|e| TerrainError::io(path, e))?;
    Ok(c.features.into_iter().map(|f| (f.id, f.geometry, f.properties)).collect())
}

fn line(path: &Path, g: Geometry) -> Result<Vec<Vec2>> {
    match g {
        Geometry::LineString { coordinates } => Ok(coordinates),
        _ => Err(TerrainError::io(path, "expected LineString")),
    }
}

impl Features {
    pub fn save(&self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir).map_err(|e| TerrainError::io(dir, e))?;
        write_collection(
            &dir.join("rivers.geojson"),
            self.rivers
                .iter()
                .map(|r| (r.id, Geometry::LineString { coordinates: r.points.clone() }, r.props.clone()))
                .collect(),
        )?;
        write_collection(
            &dir.join("roads.geojson"),
            self.roads
                .iter()
                .map(|r| (r.id, Geometry::LineString { coordinates: r.points.clone() }, r.props.clone()))
                .collect(),
        )?;
        write_collection(
            &dir.join("buildings.geojson"),
            self.buildings
                .iter()
                .map(|b| {
                    let mut ring = b.ring.clone();
                    if let Some(&first) = ring.first() {
                        ring.push(first);
                    }
                    (b.id, Geometry::Polygon { coordinates: vec![ring] }, b.props.clone())
                })
                .collect(),
        )?;
        write_collection(
            &dir.join("forests.geojson"),
            self.forests
                .iter()
                .map(|f| {
                    let mut ring = f.ring.clone();
                    if let Some(&first) = ring.first() {
                        ring.push(first); // GeoJSON rings are closed
                    }
                    (f.id, Geometry::Polygon { coordinates: vec![ring] }, f.props.clone())
                })
                .collect(),
        )
    }

    pub fn load(dir: &Path) -> Result<Self> {
        let p = dir.join("rivers.geojson");
        let rivers = read_collection::<RiverProps>(&p)?
            .into_iter()
            .map(|(id, g, props)| Ok(River { id, points: line(&p, g)?, props }))
            .collect::<Result<_>>()?;
        let p = dir.join("roads.geojson");
        let roads = read_collection::<RoadProps>(&p)?
            .into_iter()
            .map(|(id, g, props)| Ok(Road { id, points: line(&p, g)?, props }))
            .collect::<Result<_>>()?;
        let p = dir.join("forests.geojson");
        let forests = read_collection::<ForestProps>(&p)?
            .into_iter()
            .map(|(id, g, props)| match g {
                Geometry::Polygon { mut coordinates } if !coordinates.is_empty() => {
                    let mut ring = coordinates.swap_remove(0);
                    if ring.len() > 1 && ring.first() == ring.last() {
                        ring.pop();
                    }
                    Ok(Forest { id, ring, props })
                }
                _ => Err(TerrainError::io(&p, "expected Polygon")),
            })
            .collect::<Result<_>>()?;
        let p = dir.join("buildings.geojson");
        let buildings = read_collection::<BuildingProps>(&p)?
            .into_iter()
            .map(|(id, g, props)| match g {
                Geometry::Polygon { mut coordinates } if !coordinates.is_empty() => {
                    let mut ring = coordinates.swap_remove(0);
                    if ring.len() > 1 && ring.first() == ring.last() {
                        ring.pop();
                    }
                    Ok(Building { id, ring, props })
                }
                _ => Err(TerrainError::io(&p, "expected Polygon")),
            })
            .collect::<Result<_>>()?;
        Ok(Self { rivers, roads, forests, buildings })
    }

    /// Next free feature id (ids are unique across all feature kinds).
    pub fn next_id(&self) -> u32 {
        let r = self.rivers.iter().map(|f| f.id);
        let d = self.roads.iter().map(|f| f.id);
        let f = self.forests.iter().map(|f| f.id);
        let b = self.buildings.iter().map(|b| b.id);
        r.chain(d).chain(f).chain(b).max().map_or(1, |m| m + 1)
    }
}
