//! The ground surface used for sight lines and vehicle placement: baked elevation plus
//! procedural micro-relief (TERRAIN.md §2.1) and land-cover canopy for vegetation.

use sim_data::TerrainData;

use crate::{BakedMap, micro};

/// Per-class values cached from the land-cover table.
#[derive(Debug, Clone)]
struct ClassInfo {
    micro: f64,
    canopy: f64,
    k: f64,
}

pub struct Ground<'a> {
    pub map: &'a BakedMap,
    classes: Vec<ClassInfo>,
    seed: u64,
}

impl<'a> Ground<'a> {
    pub fn new(map: &'a BakedMap, td: &TerrainData) -> Self {
        let classes = td
            .landcover
            .iter()
            .map(|l| ClassInfo {
                micro: l.micro_relief_m.get(),
                canopy: l.canopy_height_m.get(),
                k: l.attenuation_per_m.get(),
            })
            .collect();
        Self { map, classes, seed: map.manifest.seed }
    }

    fn class(&self, x: f64, y: f64) -> &ClassInfo {
        let (i, j) = self.map.elevation.cell_of(x, y);
        let id = self.map.landcover.get(i, j) as usize;
        &self.classes[id.min(self.classes.len() - 1)]
    }

    /// Macro relief only (fast; used far from the ends of a sight line).
    #[inline]
    pub fn macro_height(&self, x: f64, y: f64) -> f64 {
        self.map.elevation.height(x, y)
    }

    /// Bare-earth height including micro-relief.
    pub fn height(&self, x: f64, y: f64) -> f64 {
        self.macro_height(x, y) + micro::h_micro(x, y, self.seed, self.class(x, y).micro)
    }

    /// Building height above ground at a point (0 if none).
    pub fn building(&self, x: f64, y: f64) -> f64 {
        let (i, j) = self.map.elevation.cell_of(x, y);
        self.map.building_height.get(i, j) as f64
    }

    /// (canopy height above ground, attenuation per metre) at a point.
    pub fn canopy(&self, x: f64, y: f64) -> (f64, f64) {
        let c = self.class(x, y);
        (c.canopy, c.k)
    }
}
