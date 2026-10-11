//! Procedural micro-relief (TERRAIN.md §2.1): 0.3–1.5 m relief at 5–30 m wavelengths, not
//! stored but evaluated on demand by LOS, hull-down search and vehicle pitch.

use crate::noise;

/// Micro-relief height offset (m) at a metric position. `amplitude_m` comes from the land-cover
/// class (`micro_relief_m`); the result lies roughly in `[-amplitude, amplitude]`.
pub fn h_micro(x: f64, y: f64, seed: u64, amplitude_m: f64) -> f64 {
    if amplitude_m <= 0.0 {
        return 0.0;
    }
    let s = seed ^ 0x6d69_6372_6f72_656c; // "microrel"
    let a = noise::gradient(x / 24.0, y / 24.0, s);
    let b = noise::gradient(x / 9.0, y / 9.0, s.wrapping_add(1));
    let c = noise::gradient(x / 5.0, y / 5.0, s.wrapping_add(2));
    amplitude_m * (0.6 * a + 0.3 * b + 0.1 * c)
}
