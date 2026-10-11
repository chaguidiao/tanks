//! Stage 1 — macro landform: fBm + ridged multifractal over a domain-warped plane, plus a
//! regional tilt that sets the drainage direction (TERRAIN.md §3, Musgrave; Quilez on warping).

use terrain::{Heightfield, noise};

use crate::recipe::LandformParams;

pub fn generate(p: &LandformParams, seed: u64, n: usize, cell: f64) -> Heightfield {
    let size = n as f64 * cell;
    let az = p.tilt_azimuth_deg.to_radians();
    let (tx, ty) = (libm::sin(az), libm::cos(az));
    let z = crate::par_grid(n, n, |i, j| {
        let (x, y) = ((i as f64 + 0.5) * cell, (j as f64 + 0.5) * cell);
        let wl = p.wavelength_m;
        let wx = noise::fbm(x / (wl * 0.7), y / (wl * 0.7), seed ^ 11, 3, 2.0, 0.5);
        let wy = noise::fbm(x / (wl * 0.7), y / (wl * 0.7), seed ^ 12, 3, 2.0, 0.5);
        let (qx, qy) = (x + p.warp_m * wx, y + p.warp_m * wy);
        let base = 0.5 + 0.5 * noise::fbm(qx / wl, qy / wl, seed ^ 13, p.octaves, 2.0, 0.5);
        let ridge = noise::ridged(qx / wl, qy / wl, seed ^ 14, p.octaves, 2.0, 0.5);
        let shape = (1.0 - p.ridge_mix) * base + p.ridge_mix * ridge;
        // Tilt: highest on the upslope side (towards the azimuth), lowest opposite.
        let along = ((x - size / 2.0) * tx + (y - size / 2.0) * ty) / size;
        (p.base_m + p.relief_m * shape + p.tilt_m * along) as f32
    });
    Heightfield::new(z, cell)
}
