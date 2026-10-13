//! Weather, sun and moon (DESIGN.md decision 20): turns the scenario's preset, date, latitude
//! and clock into the atmosphere the sensors see.
//!
//! - Sun elevation from solar declination (Cooper 1969) and hour angle; local time is taken as
//!   local solar time.
//! - Optical effectiveness follows `light_curve` in the sun elevation, plus the moon at night;
//!   cloud and rain (`sky_light`) only matter when it is not full daylight.
//! - Thermal contrast collapses around sunrise and sunset (crossover).
//! - Atmospheric transmittance `exp(-β·r)`; visible extinction by Koschmieder, `β = 3.912 / V`.

use ai::picture::WeatherView;
use sim_data::{DetectionModel, ScenarioDef, WeatherDef, WeatherPreset, WeatherTable};

/// The scenario's weather, resolved against the data set.
#[derive(Debug, Clone)]
pub struct Weather {
    preset: WeatherPreset,
    def: WeatherDef,
    date: (i32, u8, u8),
}

/// Atmosphere and illumination at one moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Atmosphere {
    pub sun_elevation_deg: f64,
    /// Optical sight effectiveness (1 = full daylight).
    pub light: f64,
    /// Thermal contrast factor (1 = good contrast).
    pub thermal: f64,
    pub beta_visible: f64,
    pub beta_thermal: f64,
    pub dust_cue: f64,
}

impl Weather {
    pub fn new(scenario: &ScenarioDef, table: &WeatherTable) -> Result<Self, String> {
        let preset = table
            .preset(&scenario.weather.preset)
            .ok_or_else(|| format!("unknown weather preset '{}'", scenario.weather.preset))?
            .clone();
        Ok(Self { preset, def: scenario.weather.clone(), date: scenario.date })
    }

    pub fn at(&self, clock_s: f64, m: &DetectionModel) -> Atmosphere {
        let day = libm::floor(clock_s / 86_400.0);
        let hour = (clock_s - day * 86_400.0) / 3600.0;
        let sun = sun_elevation_deg(self.def.latitude_deg, self.date, day as i32, hour);
        let sky = self.preset.sky_light.get();
        Atmosphere {
            sun_elevation_deg: sun,
            light: light_factor(sun, self.def.moon_phase, sky, m),
            thermal: thermal_factor(sun, m),
            beta_visible: 3.912 / self.preset.visibility_m.get().max(1.0),
            beta_thermal: self.preset.thermal_extinction_per_m.get(),
            dust_cue: self.preset.dust_cue.get(),
        }
    }

    pub fn view(&self, atm: &Atmosphere) -> WeatherView {
        WeatherView {
            label: self.preset.name.clone(),
            sun_elevation_deg: atm.sun_elevation_deg,
            light: atm.light,
            visibility_m: self.preset.visibility_m.get(),
            precipitation_mm_h: self.preset.precipitation_mm_h,
            wind_from_deg: self.def.wind_from_deg,
            wind_speed_mps: self.def.wind_speed_mps,
        }
    }
}

fn day_of_year(date: (i32, u8, u8)) -> i32 {
    const CUM: [i32; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let (y, mo, d) = date;
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let m = (mo.clamp(1, 12) - 1) as usize;
    CUM[m] + d as i32 + i32::from(leap && mo > 2)
}

/// Sun elevation above the horizon (deg) at local solar `hour` on `date` plus `extra_days`.
pub fn sun_elevation_deg(latitude_deg: f64, date: (i32, u8, u8), extra_days: i32, hour: f64) -> f64 {
    let n = (day_of_year(date) + extra_days) as f64;
    let decl = 23.44_f64.to_radians() * libm::sin(std::f64::consts::TAU * (n - 81.0) / 365.0);
    let h = (15.0 * (hour - 12.0)).to_radians();
    let lat = latitude_deg.to_radians();
    let s = libm::sin(lat) * libm::sin(decl) + libm::cos(lat) * libm::cos(decl) * libm::cos(h);
    libm::asin(s.clamp(-1.0, 1.0)).to_degrees()
}

/// Optical effectiveness at a sun elevation: log-linear interpolation of `light_curve`
/// (listed highest elevation first), plus moonlight and the sky factor.
pub fn light_factor(sun_deg: f64, moon_phase: f64, sky_light: f64, m: &DetectionModel) -> f64 {
    let c = &m.light_curve;
    let (Some(first), Some(last)) = (c.first(), c.last()) else { return 1.0 };
    let base = if sun_deg >= first.sun_elevation_deg {
        first.factor.get()
    } else if sun_deg <= last.sun_elevation_deg {
        last.factor.get()
    } else {
        let i = c.windows(2).position(|w| sun_deg >= w[1].sun_elevation_deg).expect("within the curve");
        let (a, b) = (&c[i], &c[i + 1]);
        let f = (a.sun_elevation_deg - sun_deg) / (a.sun_elevation_deg - b.sun_elevation_deg);
        libm::exp(libm::log(a.factor.get()) * (1.0 - f) + libm::log(b.factor.get()) * f)
    };
    // Moonlight is relative to the unlit night level, so it vanishes against daylight.
    let moon = m.moon_gain.get() * moon_phase.clamp(0.0, 1.0) * last.factor.get();
    let lit = (base + moon).min(1.0);
    lit * (sky_light + (1.0 - sky_light) * lit)
}

/// Thermal contrast around sunrise and sunset.
pub fn thermal_factor(sun_deg: f64, m: &DetectionModel) -> f64 {
    let x = &m.thermal_crossover;
    let z = (sun_deg - x.centre_elevation_deg.get()) / x.width_deg.get();
    1.0 - x.depth.get() * libm::exp(-z * z)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> DetectionModel {
        sim_data::DataSet::load(std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data")))
            .expect("data loads")
            .detection
    }

    #[test]
    fn sun_elevation_matches_the_solstice_geometry() {
        // Noon elevation = 90° − latitude + declination (±23.44° at the solstices).
        let summer = sun_elevation_deg(52.0, (2000, 6, 21), 0, 12.0);
        let winter = sun_elevation_deg(52.0, (2000, 12, 21), 0, 12.0);
        assert!((summer - 61.4).abs() < 0.6, "{summer}");
        assert!((winter - 14.6).abs() < 0.6, "{winter}");
        assert!(sun_elevation_deg(52.0, (2000, 6, 21), 0, 0.0) < -10.0);
    }

    #[test]
    fn light_falls_with_the_sun_and_the_moon_helps() {
        let m = model();
        let mut prev = f64::MAX;
        for el in [30.0, 5.0, 0.0, -4.0, -8.0, -14.0, -30.0] {
            let l = light_factor(el, 0.0, 1.0, &m);
            assert!(l <= prev, "{el}: {l}");
            prev = l;
        }
        let dark = light_factor(-30.0, 0.0, 1.0, &m);
        let moonlit = light_factor(-30.0, 1.0, 1.0, &m);
        assert!(moonlit > 3.0 * dark && moonlit < 0.2);
        assert!((light_factor(40.0, 1.0, 0.5, &m) - 1.0).abs() < 1e-9);
        assert!(light_factor(-30.0, 0.0, 0.5, &m) < dark);
    }

    #[test]
    fn thermal_contrast_dips_at_crossover() {
        let m = model();
        let dip = thermal_factor(8.0, &m);
        assert!(dip < 0.6 && thermal_factor(40.0, &m) > 0.99 && thermal_factor(-30.0, &m) > 0.99);
    }
}
