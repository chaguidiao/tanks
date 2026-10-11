//! Display-layer conversions (CLAUDE.md: SI inside, mils / km/h / grids on screen).

use std::f64::consts::TAU;

/// MGRS-style local grid reference at 100 m precision: "EEE NNN" (km and hundreds of metres).
pub fn grid(p: [f64; 2]) -> String {
    let e = (p[0].max(0.0) / 100.0).floor() as i64;
    let n = (p[1].max(0.0) / 100.0).floor() as i64;
    format!("{:03} {:03}", e % 1000, n % 1000)
}

/// 10 m precision grid: "EEEE NNNN".
pub fn grid8(p: [f64; 2]) -> String {
    let e = (p[0].max(0.0) / 10.0).floor() as i64;
    let n = (p[1].max(0.0) / 10.0).floor() as i64;
    format!("{:04} {:04}", e % 10000, n % 10000)
}

/// Compass bearing in NATO mils (6400 per circle), 4 digits.
pub fn mils(rad: f64) -> String {
    let m = (rad.rem_euclid(TAU) / TAU * 6400.0).round() as i64 % 6400;
    format!("{m:04}")
}

pub fn kmh(mps: f64) -> String {
    format!("{:.0}", mps * 3.6)
}

/// HH:MM:SS from seconds after midnight.
pub fn clock(s: f64) -> String {
    let t = s.max(0.0) as u64 % 86_400;
    format!("{:02}:{:02}:{:02}", t / 3600, t / 60 % 60, t % 60)
}

pub fn distance(m: f64) -> String {
    if m >= 1000.0 { format!("{:.1} KM", m / 1000.0) } else { format!("{:.0} M", m) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions() {
        assert_eq!(grid([4352.0, 518.0]), "043 005");
        assert_eq!(grid8([4352.0, 518.0]), "0435 0051");
        assert_eq!(mils(0.0), "0000");
        assert_eq!(mils(std::f64::consts::FRAC_PI_2), "1600");
        assert_eq!(mils(-std::f64::consts::FRAC_PI_2), "4800");
        assert_eq!(kmh(18.5), "67");
        assert_eq!(clock(21600.0 + 83.0), "06:01:23");
        assert_eq!(distance(6012.0), "6.0 KM");
    }
}
