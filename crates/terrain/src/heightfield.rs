use crate::Grid;

/// Elevation raster with metric sampling. See crate docs for the node convention.
#[derive(Debug, Clone, PartialEq)]
pub struct Heightfield {
    pub cell: f64,
    pub z: Grid<f32>,
}

impl Heightfield {
    pub fn new(z: Grid<f32>, cell: f64) -> Self {
        Self { cell, z }
    }

    pub fn width_m(&self) -> f64 {
        self.z.w as f64 * self.cell
    }

    pub fn height_m(&self) -> f64 {
        self.z.h as f64 * self.cell
    }

    /// Node coordinates of a metric position (fractional).
    #[inline]
    pub fn to_grid(&self, x: f64, y: f64) -> (f64, f64) {
        (x / self.cell - 0.5, y / self.cell - 0.5)
    }

    /// Integer cell containing a metric position, clamped to the grid.
    #[inline]
    pub fn cell_of(&self, x: f64, y: f64) -> (usize, usize) {
        let i = libm::floor(x / self.cell).clamp(0.0, (self.z.w - 1) as f64) as usize;
        let j = libm::floor(y / self.cell).clamp(0.0, (self.z.h - 1) as f64) as usize;
        (i, j)
    }

    /// Bilinear elevation at a metric position (clamped at the edges).
    pub fn height(&self, x: f64, y: f64) -> f64 {
        let (gx, gy) = self.to_grid(x, y);
        let x0 = libm::floor(gx);
        let y0 = libm::floor(gy);
        let tx = gx - x0;
        let ty = gy - y0;
        let (x0, y0) = (x0 as i64, y0 as i64);
        let h00 = self.z.get_clamped(x0, y0) as f64;
        let h10 = self.z.get_clamped(x0 + 1, y0) as f64;
        let h01 = self.z.get_clamped(x0, y0 + 1) as f64;
        let h11 = self.z.get_clamped(x0 + 1, y0 + 1) as f64;
        let a = h00 + (h10 - h00) * tx;
        let b = h01 + (h11 - h01) * tx;
        a + (b - a) * ty
    }

    /// Surface gradient (dz/dx, dz/dy) at a metric position, by central differences.
    pub fn gradient(&self, x: f64, y: f64) -> (f64, f64) {
        let d = self.cell;
        let gx = (self.height(x + d, y) - self.height(x - d, y)) / (2.0 * d);
        let gy = (self.height(x, y + d) - self.height(x, y - d)) / (2.0 * d);
        (gx, gy)
    }

    /// Gradient at a grid node (central differences, one-sided at the edges).
    pub fn node_gradient(&self, i: usize, j: usize) -> (f64, f64) {
        let z = &self.z;
        let (i0, i1) = (i.saturating_sub(1), (i + 1).min(z.w - 1));
        let (j0, j1) = (j.saturating_sub(1), (j + 1).min(z.h - 1));
        let gx = (z.get(i1, j) - z.get(i0, j)) as f64 / ((i1 - i0).max(1) as f64 * self.cell);
        let gy = (z.get(i, j1) - z.get(i, j0)) as f64 / ((j1 - j0).max(1) as f64 * self.cell);
        (gx, gy)
    }

    pub fn node_slope(&self, i: usize, j: usize) -> f64 {
        let (gx, gy) = self.node_gradient(i, j);
        libm::sqrt(gx * gx + gy * gy)
    }

    pub fn min_max(&self) -> (f32, f32) {
        self.z.data.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &v| (lo.min(v), hi.max(v)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bilinear_reproduces_a_plane() {
        // z = 2x + 3y sampled at node centres must be reproduced exactly by bilinear interpolation.
        let cell = 4.0;
        let mut g = Grid::new(16, 16, 0.0f32);
        for j in 0..16 {
            for i in 0..16 {
                let (x, y) = ((i as f64 + 0.5) * cell, (j as f64 + 0.5) * cell);
                g.set(i, j, (2.0 * x + 3.0 * y) as f32);
            }
        }
        let hf = Heightfield::new(g, cell);
        let z = hf.height(21.3, 37.9);
        assert!((z - (2.0 * 21.3 + 3.0 * 37.9)).abs() < 1e-3);
        let (gx, gy) = hf.gradient(30.0, 30.0);
        assert!((gx - 2.0).abs() < 1e-6 && (gy - 3.0).abs() < 1e-6);
    }
}
