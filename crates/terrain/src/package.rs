//! Authoring package (TERRAIN.md §2.2): the editable source of truth.
//!
//! ```text
//! maps/<id>/map.toml            manifest
//! maps/<id>/recipe.ron          generator recipe (if generated)
//! maps/<id>/rasters/*.tif       GeoTIFF layers
//! maps/<id>/features/*.geojson  vector features
//! ```

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::hash::StableHasher;
use crate::{Features, Grid, Heightfield, Result, TerrainError, geotiff};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapManifest {
    pub id: String,
    pub name: String,
    pub width_cells: usize,
    pub height_cells: usize,
    pub cell_size_m: f64,
    pub seed: u64,
    /// Preset name of the recipe that generated the map, if any.
    #[serde(default)]
    pub recipe: Option<String>,
}

impl MapManifest {
    pub fn width_m(&self) -> f64 {
        self.width_cells as f64 * self.cell_size_m
    }
    pub fn height_m(&self) -> f64 {
        self.height_cells as f64 * self.cell_size_m
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AuthoringPackage {
    pub manifest: MapManifest,
    /// Raw recipe text (RON), kept verbatim so regeneration is reproducible.
    pub recipe_ron: Option<String>,
    /// Generated or imported bare earth (m).
    pub elevation_base: Grid<f32>,
    /// Sum of editor brush deltas (m). Non-destructive: the base is never modified by brushes.
    pub edit_height: Grid<f32>,
    /// Land-cover class ids (`data/terrain/landcover.ron`).
    pub landcover: Grid<u8>,
    /// Soil class ids (`data/terrain/soils.ron`).
    pub soil: Grid<u8>,
    /// Base wetness 0–255.
    pub moisture: Grid<u8>,
    pub features: Features,
}

impl AuthoringPackage {
    pub fn new_flat(manifest: MapManifest, height: f32) -> Self {
        let (w, h) = (manifest.width_cells, manifest.height_cells);
        Self {
            manifest,
            recipe_ron: None,
            elevation_base: Grid::new(w, h, height),
            edit_height: Grid::new(w, h, 0.0),
            landcover: Grid::new(w, h, 0),
            soil: Grid::new(w, h, 0),
            moisture: Grid::new(w, h, 96),
            features: Features::default(),
        }
    }

    /// Bare earth plus brush edits (before features are carved at bake time).
    pub fn edited_elevation(&self) -> Heightfield {
        let mut z = self.elevation_base.clone();
        for (a, b) in z.data.iter_mut().zip(&self.edit_height.data) {
            *a += *b;
        }
        Heightfield::new(z, self.manifest.cell_size_m)
    }

    pub fn save(&self, dir: &Path) -> Result<()> {
        let rd = dir.join("rasters");
        std::fs::create_dir_all(&rd).map_err(|e| TerrainError::io(&rd, e))?;
        let text = toml::to_string_pretty(&self.manifest).map_err(|e| TerrainError::io(dir, e))?;
        let mp = dir.join("map.toml");
        std::fs::write(&mp, text).map_err(|e| TerrainError::io(&mp, e))?;
        if let Some(r) = &self.recipe_ron {
            let rp = dir.join("recipe.ron");
            std::fs::write(&rp, r).map_err(|e| TerrainError::io(&rp, e))?;
        }
        let c = self.manifest.cell_size_m;
        geotiff::write_f32(&rd.join("elevation_base.tif"), &self.elevation_base, c)?;
        geotiff::write_f32(&rd.join("edit_height.tif"), &self.edit_height, c)?;
        geotiff::write_u8(&rd.join("landcover.tif"), &self.landcover, c)?;
        geotiff::write_u8(&rd.join("soil.tif"), &self.soil, c)?;
        geotiff::write_u8(&rd.join("moisture.tif"), &self.moisture, c)?;
        self.features.save(&dir.join("features"))
    }

    pub fn load(dir: &Path) -> Result<Self> {
        let mp = dir.join("map.toml");
        let text = std::fs::read_to_string(&mp).map_err(|e| TerrainError::io(&mp, e))?;
        let manifest: MapManifest = toml::from_str(&text).map_err(|e| TerrainError::io(&mp, e))?;
        let recipe_ron = std::fs::read_to_string(dir.join("recipe.ron")).ok();
        let rd = dir.join("rasters");
        let pkg = Self {
            recipe_ron,
            elevation_base: geotiff::read_f32(&rd.join("elevation_base.tif"))?,
            edit_height: geotiff::read_f32(&rd.join("edit_height.tif"))?,
            landcover: geotiff::read_u8(&rd.join("landcover.tif"))?,
            soil: geotiff::read_u8(&rd.join("soil.tif"))?,
            moisture: geotiff::read_u8(&rd.join("moisture.tif"))?,
            features: Features::load(&dir.join("features"))?,
            manifest,
        };
        let (w, h) = (pkg.manifest.width_cells, pkg.manifest.height_cells);
        for (name, gw, gh) in [
            ("elevation_base", pkg.elevation_base.w, pkg.elevation_base.h),
            ("edit_height", pkg.edit_height.w, pkg.edit_height.h),
            ("landcover", pkg.landcover.w, pkg.landcover.h),
            ("soil", pkg.soil.w, pkg.soil.h),
            ("moisture", pkg.moisture.w, pkg.moisture.h),
        ] {
            if (gw, gh) != (w, h) {
                return Err(TerrainError(format!("{name}: raster is {gw}x{gh}, manifest says {w}x{h}")));
            }
        }
        Ok(pkg)
    }

    /// Hash of everything the baker reads.
    pub fn input_hash(&self) -> u64 {
        let mut h = StableHasher::new();
        let m = &self.manifest;
        h.str(&m.id);
        h.u64(m.width_cells as u64);
        h.u64(m.height_cells as u64);
        h.f64(m.cell_size_m);
        h.u64(m.seed);
        for g in [&self.elevation_base, &self.edit_height] {
            for &v in &g.data {
                h.f32(v);
            }
        }
        for g in [&self.landcover, &self.soil, &self.moisture] {
            h.write(&g.data);
        }
        let f = &self.features;
        for r in &f.rivers {
            h.u32(r.id);
            r.points.iter().for_each(|p| {
                h.f64(p[0]);
                h.f64(p[1])
            });
            r.props.width_m.iter().chain(&r.props.depth_m).for_each(|&v| h.f32(v));
        }
        for r in &f.roads {
            h.u32(r.id);
            r.points.iter().for_each(|p| {
                h.f64(p[0]);
                h.f64(p[1])
            });
            h.u8(r.props.class as u8);
            h.f32(r.props.width_m);
        }
        for b in &f.buildings {
            h.u32(b.id);
            b.ring.iter().for_each(|p| {
                h.f64(p[0]);
                h.f64(p[1])
            });
            h.f32(b.props.height_m);
            h.u8(b.props.class as u8);
        }
        for r in &f.forests {
            h.u32(r.id);
            r.ring.iter().for_each(|p| {
                h.f64(p[0]);
                h.f64(p[1])
            });
            h.u8(r.props.kind as u8);
            h.f32(r.props.height_m);
        }
        h.finish()
    }
}
