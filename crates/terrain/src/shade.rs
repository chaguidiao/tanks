//! Analysis/display images as RGBA8 byte buffers (row 0 = north, i.e. texture top). Pure CPU:
//! the client only uploads them, so no shaders are needed (CLAUDE.md rule 7).

use sim_data::TerrainData;

use crate::{Grid, Heightfield};

fn put(out: &mut [u8], k: usize, c: [u8; 3]) {
    out[k * 4..k * 4 + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
}

fn lerp3(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Iterate (i, j, k) with k the texture pixel index (north-up).
fn for_each_px(w: usize, h: usize, mut f: impl FnMut(usize, usize, usize)) {
    for j in 0..h {
        let row = h - 1 - j;
        for i in 0..w {
            f(i, j, row * w + i);
        }
    }
}

/// Muted hypsometric tint, low = dark blue-green, high = pale olive.
pub fn elevation_tint(hf: &Heightfield) -> Vec<u8> {
    const STOPS: [[f64; 3]; 4] = [[20.0, 30.0, 34.0], [38.0, 54.0, 52.0], [70.0, 80.0, 66.0], [108.0, 108.0, 90.0]];
    let (lo, hi) = hf.min_max();
    let span = ((hi - lo) as f64).max(1.0);
    let (w, h) = (hf.z.w, hf.z.h);
    let mut out = vec![0u8; w * h * 4];
    for_each_px(w, h, |i, j, k| {
        let t = ((hf.z.get(i, j) - lo) as f64 / span).clamp(0.0, 1.0) * 3.0;
        let s = (t as usize).min(2);
        let c = lerp3(STOPS[s], STOPS[s + 1], t - s as f64);
        put(&mut out, k, [c[0] as u8, c[1] as u8, c[2] as u8]);
    });
    out
}

/// Hillshade (sun from the north-west at 45°) as grey, intended for multiply blending.
/// Flat ground maps to ~200 so multiplying darkens only slopes facing away from the sun.
/// Slopes are exaggerated ×3 so gentle tactical relief stays readable.
pub fn hillshade(hf: &Heightfield) -> Vec<u8> {
    const Z_EXAGGERATION: f64 = 3.0;
    let (w, h) = (hf.z.w, hf.z.h);
    let az = 315f64.to_radians();
    let alt = 45f64.to_radians();
    // Light vector pointing towards the sun (x east, y north, z up).
    let l = [libm::sin(az) * libm::cos(alt), libm::cos(az) * libm::cos(alt), libm::sin(alt)];
    let mut out = vec![0u8; w * h * 4];
    for_each_px(w, h, |i, j, k| {
        let (gx, gy) = hf.node_gradient(i, j);
        let n = [-gx * Z_EXAGGERATION, -gy * Z_EXAGGERATION, 1.0];
        let len = libm::sqrt(n[0] * n[0] + n[1] * n[1] + 1.0);
        let d = ((n[0] * l[0] + n[1] * l[1] + n[2] * l[2]) / len).max(0.0);
        let v = (d / libm::sin(alt) * 200.0).clamp(60.0, 255.0) as u8;
        put(&mut out, k, [v, v, v]);
    });
    out
}

/// Slope classes (percent grade). Banded, not continuous, so values can be read off the legend.
pub fn slope_view(hf: &Heightfield) -> Vec<u8> {
    const BANDS: [(f64, [u8; 3]); 6] = [
        (0.05, [24, 32, 34]),
        (0.15, [44, 60, 52]),
        (0.30, [84, 92, 60]),
        (0.45, [120, 100, 52]),
        (0.60, [132, 72, 48]),
        (f64::INFINITY, [140, 44, 44]),
    ];
    let (w, h) = (hf.z.w, hf.z.h);
    let mut out = vec![0u8; w * h * 4];
    for_each_px(w, h, |i, j, k| {
        let s = hf.node_slope(i, j);
        let c = BANDS.iter().find(|b| s < b.0).map_or([140, 44, 44], |b| b.1);
        put(&mut out, k, c);
    });
    out
}

/// GO / SLOW-GO / NO-GO from a planning-speed raster relative to `reference` (quantised units).
/// NO-GO cells get a diagonal hatch so the classes differ by pattern as well as colour.
pub fn trafficability_view(speed: &Grid<u8>, reference: u8) -> Vec<u8> {
    let (w, h) = (speed.w, speed.h);
    let r = reference.max(1) as f64;
    let mut out = vec![0u8; w * h * 4];
    for_each_px(w, h, |i, j, k| {
        let v = speed.get(i, j) as f64 / r;
        let c = if v >= 0.6 {
            [38, 60, 44]
        } else if v >= 0.25 {
            if (i + j) % 4 == 0 { [70, 64, 36] } else { [96, 84, 40] }
        } else if (i + 2 * h - j) % 6 < 2 {
            [60, 24, 24]
        } else {
            [118, 40, 38]
        };
        put(&mut out, k, c);
    });
    out
}

/// Land-cover classes in their table colours; forests stippled, water flat.
pub fn landcover_view(lc: &Grid<u8>, td: &TerrainData) -> Vec<u8> {
    let (w, h) = (lc.w, lc.h);
    let mut out = vec![0u8; w * h * 4];
    for_each_px(w, h, |i, j, k| {
        let def = td.landcover(lc.get(i, j));
        let mut c = def.display_rgb;
        if def.is_forest && (i % 3 == 0 && j % 3 == 0) {
            c = [c[0] / 2, c[1] / 2 + 10, c[2] / 2];
        }
        put(&mut out, k, c);
    });
    out
}

/// Cell rectangle `[i0, i1) × [j0, j1)`.
#[derive(Debug, Clone, Copy)]
pub struct CellRect {
    pub i0: usize,
    pub j0: usize,
    pub i1: usize,
    pub j1: usize,
}

impl CellRect {
    pub fn full(w: usize, h: usize) -> Self {
        Self { i0: 0, j0: 0, i1: w, j1: h }
    }
    pub fn w(&self) -> usize {
        self.i1 - self.i0
    }
    pub fn h(&self) -> usize {
        self.j1 - self.j0
    }
}

/// Default tactical base map: hypsometric tint × hillshade with land-cover cues drawn the
/// topographic-map way (forest stipple, light village fill, flat water). `range` fixes the tint
/// scale so partial updates (editor brushes) match the rest of the image. Output rows run
/// north to south over `rect`.
pub fn tactical(
    hf: &Heightfield,
    lc: &Grid<u8>,
    water: &Grid<f32>,
    td: &TerrainData,
    range: (f32, f32),
    rect: CellRect,
) -> Vec<u8> {
    const STOPS: [[f64; 3]; 4] = [[20.0, 30.0, 34.0], [38.0, 54.0, 52.0], [70.0, 80.0, 66.0], [108.0, 108.0, 90.0]];
    const Z_EXAGGERATION: f64 = 3.0;
    let (lo, hi) = range;
    let span = ((hi - lo) as f64).max(1.0);
    let az = 315f64.to_radians();
    let alt = 45f64.to_radians();
    let l = [libm::sin(az) * libm::cos(alt), libm::cos(az) * libm::cos(alt), libm::sin(alt)];
    let (w, h) = (rect.w(), rect.h());
    let mut out = vec![0u8; w * h * 4];
    for j in rect.j0..rect.j1 {
        let row = rect.j1 - 1 - j;
        for i in rect.i0..rect.i1 {
            let k = row * w + (i - rect.i0);
            let t = ((hf.z.get(i, j) - lo) as f64 / span).clamp(0.0, 1.0) * 3.0;
            let s = (t as usize).min(2);
            let mut c = lerp3(STOPS[s], STOPS[s + 1], t - s as f64);
            let (gx, gy) = hf.node_gradient(i, j);
            let n = [-gx * Z_EXAGGERATION, -gy * Z_EXAGGERATION, 1.0];
            let len = libm::sqrt(n[0] * n[0] + n[1] * n[1] + 1.0);
            let d = ((n[0] * l[0] + n[1] * l[1] + n[2] * l[2]) / len).max(0.0);
            let shade = (d / libm::sin(alt)).clamp(0.3, 1.25);
            c = [c[0] * shade, c[1] * shade, c[2] * shade];
            let def = td.landcover(lc.get(i, j));
            if water.get(i, j) > 0.05 || def.is_water {
                c = [44.0, 66.0, 84.0];
            } else if def.is_forest {
                c = [c[0] * 0.62 + 2.0, c[1] * 0.78 + 8.0, c[2] * 0.62 + 2.0];
                if i % 3 == 0 && j % 3 == 0 {
                    c = [c[0] * 0.45, c[1] * 0.6, c[2] * 0.45];
                }
            } else if def.built_up {
                c = [c[0] * 0.6 + 40.0, c[1] * 0.6 + 38.0, c[2] * 0.6 + 36.0];
            } else if def.name == "marsh" && (i + j) % 4 == 0 {
                c = [c[0] * 0.8, c[1] * 0.9, c[2] * 1.05];
            }
            put(&mut out, k, [c[0].min(255.0) as u8, c[1].min(255.0) as u8, c[2].min(255.0) as u8]);
        }
    }
    out
}
