//! Deterministic 2D gradient noise and fractal sums. Pure integer hashing plus `libm`-free
//! arithmetic (only `floor`), so results are identical on every platform.

#[inline]
pub fn hash_u64(mut x: u64) -> u64 {
    // SplitMix64 finaliser.
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

#[inline]
pub fn hash2(ix: i64, iy: i64, seed: u64) -> u64 {
    hash_u64(seed ^ hash_u64((ix as u64).wrapping_mul(0x9e37_79b9) ^ (iy as u64).rotate_left(32)))
}

/// Uniform value in [0, 1) from a lattice point.
#[inline]
pub fn hash01(ix: i64, iy: i64, seed: u64) -> f64 {
    (hash2(ix, iy, seed) >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
}

const H: f64 = std::f64::consts::FRAC_1_SQRT_2;

// 16 unit gradients at 22.5° steps (cos, sin), written out so no trig is evaluated.
const GRADS: [(f64, f64); 16] = [
    (1.0, 0.0),
    (0.923_879_532_5, 0.382_683_432_4),
    (H, H),
    (0.382_683_432_4, 0.923_879_532_5),
    (0.0, 1.0),
    (-0.382_683_432_4, 0.923_879_532_5),
    (-H, H),
    (-0.923_879_532_5, 0.382_683_432_4),
    (-1.0, 0.0),
    (-0.923_879_532_5, -0.382_683_432_4),
    (-H, -H),
    (-0.382_683_432_4, -0.923_879_532_5),
    (0.0, -1.0),
    (0.382_683_432_4, -0.923_879_532_5),
    (H, -H),
    (0.923_879_532_5, -0.382_683_432_4),
];

#[inline]
fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[inline]
fn floor(x: f64) -> f64 {
    let t = x as i64 as f64;
    if t > x { t - 1.0 } else { t }
}

/// Perlin-style gradient noise, roughly in [-1, 1].
pub fn gradient(x: f64, y: f64, seed: u64) -> f64 {
    let x0 = floor(x);
    let y0 = floor(y);
    let (fx, fy) = (x - x0, y - y0);
    let (ix, iy) = (x0 as i64, y0 as i64);
    let dot = |cx: i64, cy: i64, dx: f64, dy: f64| {
        let g = GRADS[(hash2(cx, cy, seed) & 15) as usize];
        g.0 * dx + g.1 * dy
    };
    let n00 = dot(ix, iy, fx, fy);
    let n10 = dot(ix + 1, iy, fx - 1.0, fy);
    let n01 = dot(ix, iy + 1, fx, fy - 1.0);
    let n11 = dot(ix + 1, iy + 1, fx - 1.0, fy - 1.0);
    let (u, v) = (fade(fx), fade(fy));
    let a = n00 + (n10 - n00) * u;
    let b = n01 + (n11 - n01) * u;
    (a + (b - a) * v) * 1.414
}

/// Fractional Brownian motion. `x, y` are already scaled to the base frequency.
pub fn fbm(x: f64, y: f64, seed: u64, octaves: u32, lacunarity: f64, gain: f64) -> f64 {
    let (mut sum, mut amp, mut norm, mut f) = (0.0, 1.0, 0.0, 1.0);
    for o in 0..octaves {
        sum += amp * gradient(x * f, y * f, seed.wrapping_add(o as u64 * 0x51ed_270b));
        norm += amp;
        amp *= gain;
        f *= lacunarity;
    }
    sum / norm
}

/// Ridged multifractal (Musgrave), in [0, 1]: sharp crests where the base noise crosses zero.
pub fn ridged(x: f64, y: f64, seed: u64, octaves: u32, lacunarity: f64, gain: f64) -> f64 {
    let (mut sum, mut amp, mut norm, mut f, mut weight) = (0.0, 1.0, 0.0, 1.0, 1.0);
    for o in 0..octaves {
        let n = gradient(x * f, y * f, seed.wrapping_add(o as u64 * 0x2545_f491));
        let mut r = 1.0 - n.abs();
        r *= r;
        r *= weight;
        weight = (r * 2.0).clamp(0.0, 1.0);
        sum += r * amp;
        norm += amp;
        amp *= gain;
        f *= lacunarity;
    }
    sum / norm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_is_deterministic_and_bounded() {
        let mut lo: f64 = 0.0;
        let mut hi: f64 = 0.0;
        for i in 0..2000 {
            let (x, y) = (i as f64 * 0.37 - 300.0, i as f64 * 0.91 - 100.0);
            let n = gradient(x, y, 7);
            assert_eq!(n, gradient(x, y, 7));
            lo = lo.min(n);
            hi = hi.max(n);
        }
        assert!(lo >= -1.2 && hi <= 1.2 && hi - lo > 1.0, "range {lo}..{hi}");
        // Value at lattice points is zero for gradient noise.
        assert_eq!(gradient(3.0, -5.0, 1), 0.0);
    }
}
