//! Terrain: map package format (authoring + baked), heightfield, contours, mobility rasters,
//! pathfinding and analysis images. No graphics dependencies (CLAUDE.md rule 1).
//!
//! Coordinates: local planar metres, origin at the south-west corner, x east, y north.
//! Grid row 0 is the southern row; raster node `(i, j)` sits at the cell centre
//! `((i + 0.5) * cell, (j + 0.5) * cell)`.

pub mod bake;
pub mod contour;
pub mod features;
pub mod geotiff;
pub mod grid;
pub mod ground;
pub mod hash;
pub mod heightfield;
pub mod los;
pub mod micro;
pub mod mobility;
pub mod noise;
pub mod package;
pub mod path;
pub mod png;
pub mod raster;
pub mod shade;

pub use bake::{BakedMap, bake};
pub use features::Features;
pub use grid::Grid;
pub use heightfield::Heightfield;
pub use package::{AuthoringPackage, MapManifest};

pub type Vec2 = [f64; 2];

#[derive(Debug)]
pub struct TerrainError(pub String);

impl std::fmt::Display for TerrainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TerrainError {}

impl TerrainError {
    pub fn io(path: &std::path::Path, e: impl std::fmt::Display) -> Self {
        Self(format!("{}: {e}", path.display()))
    }
}

pub type Result<T> = std::result::Result<T, TerrainError>;
