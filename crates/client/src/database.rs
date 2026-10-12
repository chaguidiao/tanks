//! DATABASE screen (DESIGN.md §10.4): the vehicle, ammunition, terrain, doctrine and combat-model
//! data exactly as the simulation loads it from `data/`, with each value's source and
//! confidence, plus a few values derived with the sim's own formulas (marked CALC).
//!
//! Sheets are plain data built from the `DataSet` (testable without a window); drawing lays
//! them out as a scrolling table. Opened from the main menu, the screen menu, or the game's
//! selected-vehicle card (I).

use sim::gunnery;
use sim_data::{AmmoDef, Confidence, DataSet, DoctrinePack, LandCoverDef, ServiceDates, SoilDef, Stat, VehicleDef};

use crate::hud::{self, Button, Look, Rect};
use crate::menu::{self, Backdrop};
use crate::render::{Frame, Input, Key, P2, Rgba};
use crate::style;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Vehicles,
    Ammunition,
    Terrain,
    Doctrine,
    Combat,
}

impl Tab {
    pub const ALL: [Tab; 5] = [Tab::Vehicles, Tab::Ammunition, Tab::Terrain, Tab::Doctrine, Tab::Combat];

    fn label(self) -> &'static str {
        match self {
            Tab::Vehicles => "VEHICLES",
            Tab::Ammunition => "AMMUNITION",
            Tab::Terrain => "TERRAIN",
            Tab::Doctrine => "DOCTRINE",
            Tab::Combat => "COMBAT MODELS",
        }
    }

    fn index(self) -> usize {
        Tab::ALL.iter().position(|t| *t == self).expect("listed")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbAction {
    Tab(Tab),
    Pick(usize),
    Back,
}

/// Where a value comes from: a data file's `Stat` or a formula over them.
#[derive(Debug, Clone, PartialEq)]
pub enum Origin {
    /// A plain fact (name, flag, choice).
    Fact,
    Stat {
        confidence: Confidence,
        source: String,
        notes: Option<String>,
    },
    /// Derived with the given formula.
    Calc(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    Section(String),
    Row {
        label: String,
        value: String,
        origin: Origin,
    },
    Text(String),
    /// Display colour of a land-cover class.
    Swatch([u8; 3]),
    /// Formation diagrams: name and slots (right, forward) in spacing units.
    Formations(Vec<(String, Vec<(f64, f64)>)>),
}

impl Line {
    fn height(&self) -> f32 {
        match self {
            Line::Section(_) => 34.0,
            Line::Row { .. } | Line::Text(_) => 20.0,
            Line::Swatch(_) => 26.0,
            Line::Formations(_) => 120.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sheet {
    pub title: String,
    pub sub: String,
    pub lines: Vec<Line>,
}

pub struct Database {
    pub tab: Tab,
    /// Selected entry per tab.
    sel: [usize; 5],
    scroll: f32,
    /// Opened from the screen menu: BACK returns to it.
    pub from_pause: bool,
}

// ---------------------------------------------------------------- sheet building

fn stat(label: &str, s: &Stat<f64>, fmt: impl Fn(f64) -> String) -> Line {
    Line::Row {
        label: label.to_owned(),
        value: fmt(s.value),
        origin: Origin::Stat { confidence: s.confidence, source: s.source.clone(), notes: s.notes.clone() },
    }
}

fn fact(label: &str, value: impl Into<String>) -> Line {
    Line::Row { label: label.to_owned(), value: value.into(), origin: Origin::Fact }
}

fn calc(label: &str, value: impl Into<String>, formula: &str) -> Line {
    Line::Row { label: label.to_owned(), value: value.into(), origin: Origin::Calc(formula.to_owned()) }
}

fn section(s: &str) -> Line {
    Line::Section(s.to_owned())
}

fn years(d: &ServiceDates) -> String {
    match d.to {
        Some(t) => format!("{}–{t}", d.from),
        None => format!("{}–", d.from),
    }
}

fn yes(b: bool) -> &'static str {
    if b { "YES" } else { "NO" }
}

fn m(v: f64) -> String {
    format!("{v:.2} M")
}

fn kmh(v: f64) -> String {
    format!("{:.0} KM/H", v * 3.6)
}

fn pct(v: f64) -> String {
    format!("{:.0} %", v * 100.0)
}

fn secs(v: f64) -> String {
    format!("{v:.1} S")
}

fn mrad(v: f64) -> String {
    format!("{v:.2} MRAD (1 SD)")
}

fn deg(v: f64) -> String {
    format!("{:+.1}°", v.to_degrees())
}

fn mm(v: f64) -> String {
    format!("{v:.0} MM")
}

fn plain(v: f64) -> String {
    format!("{v:.2}")
}

fn upper(s: &str) -> String {
    s.replace(['_', '-'], " ").to_uppercase()
}

/// Entries of a tab: (label, value) rows of the list.
pub fn entries(ds: &DataSet, tab: Tab) -> Vec<(String, String)> {
    match tab {
        Tab::Vehicles => ds.vehicles.iter().map(|v| (v.name.clone(), years(&v.in_service))).collect(),
        Tab::Ammunition => ds.ammo.iter().map(|a| (a.name.clone(), years(&a.in_service))).collect(),
        Tab::Terrain => ds
            .terrain
            .landcover
            .iter()
            .map(|l| (upper(&l.name), "LAND COVER".to_owned()))
            .chain(ds.terrain.soils.iter().map(|s| (upper(&s.name), "SOIL".to_owned())))
            .collect(),
        Tab::Doctrine => ds.doctrine.iter().map(|d| (d.name.to_uppercase(), d.id.to_uppercase())).collect(),
        Tab::Combat => {
            ["DETECTION", "PENETRATION", "DAMAGE"].iter().map(|s| ((*s).to_owned(), String::new())).collect()
        }
    }
}

pub fn sheet(ds: &DataSet, tab: Tab, k: usize) -> Option<Sheet> {
    Some(match tab {
        Tab::Vehicles => vehicle_sheet(ds, ds.vehicles.get(k)?),
        Tab::Ammunition => ammo_sheet(ds, ds.ammo.get(k)?),
        Tab::Terrain => {
            let n = ds.terrain.landcover.len();
            if k < n { landcover_sheet(&ds.terrain.landcover[k]) } else { soil_sheet(ds, ds.terrain.soils.get(k - n)?) }
        }
        Tab::Doctrine => doctrine_sheet(ds.doctrine.get(k)?),
        Tab::Combat => combat_sheet(ds, k)?,
    })
}

fn vehicle_sheet(ds: &DataSet, v: &VehicleDef) -> Sheet {
    let mo = &v.mobility;
    let mass_t = mo.combat_mass_kg.get() / 1000.0;
    let kw = mo.engine_power_w.get() / 1000.0;
    let pressure_kpa = mo.combat_mass_kg.get() * gunnery::G
        / (2.0 * mo.track_ground_contact_m.get() * mo.track_width_m.get())
        / 1000.0;
    let a = &v.armament;
    let fc = &v.fire_control;
    let mut l = vec![
        section("DIMENSIONS"),
        stat("HULL LENGTH", &v.dimensions.hull_length_m, m),
        stat("WIDTH", &v.dimensions.width_m, m),
        stat("HEIGHT", &v.dimensions.height_m, m),
        stat("HULL HEIGHT (HULL-DOWN)", &v.dimensions.hull_height_m, m),
        stat("SIGHT HEIGHT", &v.dimensions.sight_height_m, m),
        section("MOBILITY"),
        stat("COMBAT MASS", &mo.combat_mass_kg, |x| format!("{:.1} T", x / 1000.0)),
        stat("ENGINE", &mo.engine_power_w, |x| format!("{:.0} KW ({:.0} HP)", x / 1000.0, x / 745.7)),
        calc("POWER TO WEIGHT", format!("{:.1} KW/T ({:.1} HP/T)", kw / mass_t, kw / 0.7457 / mass_t), "ENGINE / MASS"),
        stat("DRIVETRAIN EFFICIENCY", &mo.drivetrain_efficiency, plain),
        stat("ROAD SPEED (GOVERNED)", &mo.max_speed_road_mps, kmh),
        stat("CROSS-COUNTRY SPEED", &mo.max_speed_cross_country_mps, kmh),
        stat("REVERSE SPEED", &mo.max_speed_reverse_mps, kmh),
        stat("GRADIENT", &mo.max_slope, pct),
        stat("SIDE SLOPE", &mo.max_side_slope, pct),
        stat("FORDING (UNPREPARED)", &mo.fording_depth_m, m),
        stat("VERTICAL STEP", &mo.vertical_step_m, m),
        stat("TRENCH", &mo.trench_width_m, m),
        calc(
            "GROUND PRESSURE",
            format!("{pressure_kpa:.0} KPA"),
            "MASS · G / (2 · TRACK CONTACT LENGTH · TRACK WIDTH)",
        ),
        stat("PIVOT RATE", &mo.max_yaw_rate_rad_s, |x| format!("{:.0} °/S", x.to_degrees())),
        stat("LATERAL ACCELERATION", &mo.max_lateral_accel_mps2, |x| format!("{x:.1} M/S²")),
        stat("BRAKING", &mo.braking_decel_mps2, |x| format!("{x:.1} M/S²")),
        stat("FUEL", &mo.fuel_capacity_l, |x| format!("{x:.0} L")),
        stat("FUEL AT IDLE", &mo.fuel_idle_lph, |x| format!("{x:.0} L/H")),
        stat("FUEL PER KWH", &mo.fuel_per_kwh_l, |x| format!("{x:.2} L/KWH")),
        calc("FUEL AT FULL POWER", format!("{:.0} L/H", kw * mo.fuel_per_kwh_l.get()), "ENGINE KW · FUEL PER KWH"),
        section("ARMAMENT"),
        fact("MAIN GUN", a.gun.clone()),
        stat("ELEVATION", &v.gun.elevation_rad, deg),
        stat("DEPRESSION", &v.gun.depression_rad, deg),
    ];
    for ld in &a.load {
        let name = ds.ammo(&ld.ammo).map_or(ld.ammo.clone(), |x| x.name.clone());
        l.push(fact("LOAD", format!("{} × {name}", ld.count)));
    }
    l.extend([
        fact("LOADING", if a.autoloader { "AUTOLOADER" } else { "HUMAN LOADER" }),
        stat("RELOAD", &a.reload_s, secs),
        stat("TURRET TRAVERSE", &a.turret_traverse_rad_s, |x| format!("{:.0} °/S", x.to_degrees())),
        calc("FULL TRAVERSE", secs(std::f64::consts::TAU / a.turret_traverse_rad_s.get()), "360° / TRAVERSE RATE"),
        stat("ENGAGEMENT RANGE (CREW)", &a.max_engagement_range_m, |x| format!("{x:.0} M")),
        fact(
            "AMMUNITION STOWAGE",
            match v.ammo_stowage {
                sim_data::AmmoStowage::Isolated => "ISOLATED (BLOW-OUT PANELS)",
                sim_data::AmmoStowage::Exposed => "EXPOSED (HIGH COOK-OFF RISK)",
            },
        ),
        section("FIRE CONTROL"),
        fact("LASER RANGEFINDER", yes(fc.laser_rangefinder)),
        stat("LASER RANGE ERROR", &fc.laser_range_error_m, |x| format!("{x:.0} M (1 SD)")),
        stat("ESTIMATED RANGE ERROR", &fc.estimated_range_error_fraction, |x| {
            format!("{:.0} % OF RANGE (1 SD)", x * 100.0)
        }),
        stat("BALLISTIC ERROR", &fc.ballistic_error_mrad, mrad),
        stat("AIM ERROR, HALTED", &fc.aim_error_mrad, mrad),
        stat("AIM ERROR, MOVING", &fc.moving_error_mrad, mrad),
        stat("LEAD ERROR", &fc.lead_error_fraction, |x| format!("{:.0} % OF LEAD", x * 100.0)),
        stat("LAY TIME", &fc.lay_time_s, secs),
        stat("LASE TIME", &fc.lase_time_s, secs),
        section("SENSORS"),
    ]);
    for s in &v.sensors {
        let kind = match s.kind {
            sim_data::SensorKind::Optical => "OPTICAL",
            sim_data::SensorKind::NightChannel => "NIGHT CHANNEL",
            sim_data::SensorKind::Thermal => "THERMAL",
        };
        let reach = s.max_range_m.as_ref().map_or(String::new(), |r| format!(" · MAX {:.0} M", r.value));
        l.push(stat(&s.name, &s.cycles_per_mrad, |x| {
            format!("{kind} · {x:.1} CYCLES/MRAD · {}{reach}", years(&s.in_service))
        }));
    }
    l.extend([
        section("ARMOUR (RHA EQUIVALENT VS KE, NORMAL ATTACK)"),
        stat("TURRET FRONT", &v.armour.turret_front, mm),
        stat("TURRET SIDE", &v.armour.turret_side, mm),
        stat("TURRET REAR", &v.armour.turret_rear, mm),
        stat("HULL FRONT", &v.armour.hull_front, mm),
        stat("HULL SIDE", &v.armour.hull_side, mm),
        stat("HULL REAR", &v.armour.hull_rear, mm),
    ]);
    let threats: Vec<&AmmoDef> = ds.ammo.iter().collect();
    if !threats.is_empty() {
        l.push(section("FRONTAL ARMOUR DEFEATED BY"));
        for t in threats {
            l.push(calc(&t.name, defeat_text(ds, t, v), "LANZ–ODERMATT PENETRATION AT REMAINING VELOCITY ≥ ARMOUR"));
        }
    }
    Sheet {
        title: v.name.clone(),
        sub: format!("MIRRORS {} · IN SERVICE {} · CREW {}", v.mirrors.to_uppercase(), years(&v.in_service), v.crew),
        lines: l,
    }
}

/// Longest range (to 5 km, 50 m steps) at which `a` still penetrates `armour` mm; None if never.
fn defeat_range(ds: &DataSet, a: &AmmoDef, armour: f64) -> Option<f64> {
    (0..=100)
        .rev()
        .map(|k| k as f64 * 50.0)
        .find(|&r| gunnery::penetration_mm(&ds.penetration, a, gunnery::velocity_at(a, r)) >= armour)
}

fn range_text(r: Option<f64>) -> String {
    match r {
        None => "NO".into(),
        Some(r) if r >= 5000.0 => "ANY RANGE".into(),
        Some(r) => format!("TO {:.1} KM", r / 1000.0),
    }
}

fn defeat_text(ds: &DataSet, a: &AmmoDef, v: &VehicleDef) -> String {
    format!(
        "TURRET FRONT {}  ·  HULL FRONT {}",
        range_text(defeat_range(ds, a, v.armour.turret_front.get())),
        range_text(defeat_range(ds, a, v.armour.hull_front.get()))
    )
}

fn ammo_sheet(ds: &DataSet, a: &AmmoDef) -> Sheet {
    let mut l = vec![
        section("PROJECTILE"),
        stat("MUZZLE VELOCITY", &a.muzzle_velocity_mps, |x| format!("{x:.0} M/S")),
        stat("VELOCITY LOSS", &a.velocity_loss_mps_per_km, |x| format!("{x:.0} M/S PER KM")),
        stat("ROD LENGTH", &a.rod_length_mm, mm),
        stat("ROD DIAMETER", &a.rod_diameter_mm, mm),
        calc("LENGTH / DIAMETER", format!("{:.0}", a.rod_length_mm.get() / a.rod_diameter_mm.get()), "L / D"),
        stat("ROD DENSITY", &a.rod_density_g_cm3, |x| format!("{x:.1} G/CM³")),
        stat("DISPERSION", &a.dispersion_mrad, mrad),
    ];
    if let Some(p) = &a.reference_penetration_2km_mm {
        l.push(stat("PUBLISHED PENETRATION, 2 KM", p, |x| format!("{x:.0} MM RHA (VALIDATION ONLY)")));
    }
    l.push(section("FLIGHT AND PENETRATION (SIM MODEL, RHA AT NORMAL)"));
    for r in [0.0, 1000.0, 2000.0, 3000.0, 4000.0] {
        let v = gunnery::velocity_at(a, r);
        l.push(calc(
            &format!("AT {:.0} KM", r / 1000.0),
            format!(
                "{v:.0} M/S  ·  FLIGHT {:.2} S  ·  DROP {:.1} M  ·  {:.0} MM",
                gunnery::time_of_flight(a, r),
                gunnery::drop_at(a, r),
                gunnery::penetration_mm(&ds.penetration, a, v)
            ),
            "VELOCITY: LINEAR LOSS; DROP: G·T²/2; PENETRATION: LANZ–ODERMATT (COMBAT MODELS)",
        ));
    }
    l.push(section("DEFEATS FRONTAL ARMOUR OF"));
    for v in &ds.vehicles {
        l.push(calc(&v.name, defeat_text(ds, a, v), "LANZ–ODERMATT PENETRATION AT REMAINING VELOCITY ≥ ARMOUR"));
    }
    l.push(section("CARRIED BY"));
    for v in ds.vehicles.iter().filter(|v| v.armament.load.iter().any(|x| x.ammo == a.id)) {
        let n: u32 = v.armament.load.iter().filter(|x| x.ammo == a.id).map(|x| x.count).sum();
        l.push(fact(&v.name, format!("{n} ROUNDS · {}", v.armament.gun)));
    }
    let kind = match a.kind {
        sim_data::AmmoKind::Apfsds => "APFSDS",
    };
    Sheet {
        title: a.name.clone(),
        sub: format!("MIRRORS {} · {kind} · IN SERVICE {}", a.mirrors.to_uppercase(), years(&a.in_service)),
        lines: l,
    }
}

fn landcover_sheet(c: &LandCoverDef) -> Sheet {
    let half = std::f64::consts::LN_2 / c.attenuation_per_m.get().max(1e-9);
    let mut l = vec![
        Line::Swatch(c.display_rgb),
        section("MOBILITY"),
        stat("SPEED FACTOR", &c.speed_factor, |x| if x <= 0.0 { "NO-GO".into() } else { format!("× {x:.2}") }),
        stat("MICRO-RELIEF", &c.micro_relief_m, m),
        section("SIGHT"),
        stat("CANOPY / OBSTRUCTION HEIGHT", &c.canopy_height_m, |x| format!("{x:.1} M")),
        stat("ATTENUATION IN CANOPY", &c.attenuation_per_m, |x| format!("{x:.3} PER M")),
    ];
    if c.canopy_height_m.get() > 0.0 && c.attenuation_per_m.get() > 0.0 {
        l.push(calc("HALF THE VIEW LOST AFTER", format!("{half:.0} M OF CANOPY"), "LN 2 / ATTENUATION (BEER–LAMBERT)"));
    }
    l.extend([section("CLASS"), fact("FOREST", yes(c.is_forest)), fact("WATER", yes(c.is_water))]);
    Sheet {
        title: upper(&c.name),
        sub: format!("LAND COVER CLASS {} · RASTER VALUE {}", upper(&c.name), c.id),
        lines: l,
    }
}

fn soil_sheet(ds: &DataSet, s: &SoilDef) -> Sheet {
    let mut l = vec![
        section("TRACKED VEHICLES"),
        stat("ROLLING RESISTANCE", &s.rolling_resistance, plain),
        stat("TRACTION COEFFICIENT", &s.traction_coefficient, plain),
        calc(
            "STEEPEST GRADE BY TRACTION",
            pct((s.traction_coefficient.get() - s.rolling_resistance.get()).max(0.0)),
            "TRACTION − ROLLING RESISTANCE",
        ),
        section("POWER-LIMITED SPEED (CAPPED AT CROSS-COUNTRY MAXIMUM)"),
    ];
    for v in &ds.vehicles {
        let mo = &v.mobility;
        let at = |g: f64| {
            sim::mobility::power_limited_speed(
                mo.engine_power_w.get(),
                mo.drivetrain_efficiency.get(),
                mo.combat_mass_kg.get(),
                s.rolling_resistance.get(),
                g,
            )
            .min(mo.max_speed_cross_country_mps.get())
        };
        l.push(calc(
            &v.name,
            format!("LEVEL {}  ·  10 % {}  ·  30 % {}", kmh(at(0.0)), kmh(at(0.1)), kmh(at(0.3))),
            "ETA·P / (M·G·(F·COS A + SIN A)), A = GRADE ANGLE  (WONG, THEORY OF GROUND VEHICLES)",
        ));
    }
    Sheet { title: upper(&s.name), sub: format!("SOIL CLASS · RASTER VALUE {}", s.id), lines: l }
}

fn formation(f: sim_data::Formation) -> String {
    upper(&format!("{f:?}").chars().fold(String::new(), |mut s, c| {
        if c.is_uppercase() && !s.is_empty() {
            s.push(' ');
        }
        s.push(c);
        s
    }))
}

fn doctrine_sheet(d: &DoctrinePack) -> Sheet {
    let l = vec![
        section("MOVEMENT"),
        fact("DEFAULT FORMATION", formation(d.default_formation)),
        stat("VEHICLE INTERVAL", &d.spacing_m, |x| format!("{x:.0} M")),
        stat("BOUND LENGTH", &d.bound_length_m, |x| format!("{x:.0} M")),
        stat("OVERWATCH DISTANCE", &d.overwatch_distance_m, |x| format!("{x:.0} M")),
        Line::Formations(d.formations.iter().map(|f| (formation(f.formation), f.slots.clone())).collect()),
        section("COMMAND AND REPORTING"),
        stat("SPOT REPORT DELAY, MIN", &d.report_delay_min_s, secs),
        stat("SPOT REPORT DELAY, MAX", &d.report_delay_max_s, secs),
        stat("ORDER DELAY, MIN", &d.order_delay_min_s, secs),
        stat("ORDER DELAY, MAX", &d.order_delay_max_s, secs),
        section("ENGAGEMENT"),
        stat("OPEN FIRE INSIDE", &d.engagement_range_m, |x| format!("{x:.0} M (WEAPONS FREE)")),
        fact("REACT TO CONTACT", if d.halt_on_contact { "HALT AND FIGHT" } else { "CONTINUE MOVING" }),
        stat("BATTLE POSITION DISPERSION", &d.bp_dispersion_m, |x| format!("{x:.0} M")),
        stat("BREAK OFF AFTER LOSING", &d.break_off_losses, pct),
        section("ATTACK TEMPLATE (COMMANDER AI)"),
        fact("MARCH FORMATION", formation(d.march_formation)),
        stat("PRE-BATTLE DEPLOYMENT AT", &d.prebattle_distance_m, |x| format!("{:.1} KM FROM OBJECTIVE", x / 1000.0)),
        fact("PRE-BATTLE FORMATION", formation(d.prebattle_formation)),
        stat("BATTLE DEPLOYMENT AT", &d.battle_distance_m, |x| format!("{:.1} KM FROM OBJECTIVE", x / 1000.0)),
        fact("BATTLE FORMATION", formation(d.battle_formation)),
    ];
    Sheet {
        title: d.name.to_uppercase(),
        sub: format!("PACK {} · {}", d.id.to_uppercase(), d.source.to_uppercase()),
        lines: l,
    }
}

fn combat_sheet(ds: &DataSet, k: usize) -> Option<Sheet> {
    Some(match k {
        0 => {
            let d = &ds.detection;
            Sheet {
                title: "DETECTION".into(),
                sub: "JOHNSON CRITERIA / TTP CYCLES ACROSS THE TARGET, ACQUIRE SEARCH TIME".into(),
                lines: vec![
                    section("CYCLES FOR 50 % PROBABILITY (N50)"),
                    stat("DETECT", &d.n50_detect, plain),
                    stat("RECOGNISE (TANK VS OTHER)", &d.n50_recognise, plain),
                    stat("IDENTIFY (WHICH TANK)", &d.n50_identify, plain),
                    section("SEARCH"),
                    stat("ACQUIRE TIME CONSTANT", &d.acquire_tau_s, secs),
                    stat("TARGET MOVING", &d.moving_target_factor, |x| format!("× {x:.2}")),
                    stat("TARGET FIRING", &d.firing_target_factor, |x| format!("× {x:.2}")),
                    stat("OBSERVER MOVING", &d.observer_moving_factor, |x| format!("× {x:.2}")),
                    section("OPTICAL SIGHTS BY LIGHT"),
                    stat("DAY", &d.light_day, |x| format!("× {x:.2}")),
                    stat("TWILIGHT", &d.light_twilight, |x| format!("× {x:.2}")),
                    stat("NIGHT", &d.light_night, |x| format!("× {x:.2}")),
                    section("LIMITS"),
                    stat("DETECTION CHECKS OUT TO", &d.max_range_m, |x| format!("{x:.0} M")),
                    stat("TRACK DROPPED AFTER", &d.track_timeout_s, secs),
                ],
            }
        }
        1 => {
            let p = &ds.penetration;
            Sheet {
                title: "PENETRATION".into(),
                sub: "LONG-ROD PENETRATION INTO RHA (LANZ–ODERMATT FORM)".into(),
                lines: vec![
                    Line::Text(
                        "P / L = A · COTH(B0 + B1 · L/D) · SQRT(RHO-P / RHO-T) · EXP(−C · BHN² / (RHO-P · V²))".into(),
                    ),
                    section("CONSTANTS"),
                    stat("A", &p.a, |x| format!("{x:.3}")),
                    stat("B0", &p.b0, |x| format!("{x:.3}")),
                    stat("B1", &p.b1, |x| format!("{x:.4}")),
                    stat("C", &p.c, |x| format!("{x:.3}")),
                    section("TARGET"),
                    stat("HARDNESS", &p.target_bhn, |x| format!("{x:.0} BHN")),
                    stat("DENSITY", &p.target_density_g_cm3, |x| format!("{x:.2} G/CM³")),
                ],
            }
        }
        2 => {
            let d = &ds.damage;
            let mut l = vec![Line::Text("PROBABILITY OF EACH OUTCOME PER HIT; THE REST HAS NO FURTHER EFFECT.".into())];
            for (name, o) in [
                ("TURRET PENETRATION", &d.turret_penetration),
                ("HULL FRONT PENETRATION", &d.hull_front_penetration),
                ("HULL SIDE / REAR PENETRATION", &d.hull_side_rear_penetration),
                ("NO PENETRATION", &d.non_penetration),
            ] {
                l.extend([
                    section(name),
                    stat("CATASTROPHIC KILL", &o.catastrophic, pct),
                    stat("FIREPOWER KILL", &o.firepower, pct),
                    stat("MOBILITY KILL", &o.mobility, pct),
                    stat("CREW CASUALTIES", &o.crew, pct),
                ]);
            }
            l.extend([
                section("MODIFIERS"),
                stat("EXPOSED STOWAGE, CATASTROPHIC", &d.exposed_stowage_factor, |x| format!("× {x:.1}")),
                stat("SIGHTS DAMAGED BY A NON-PENETRATION", &d.non_penetration_sights, pct),
            ]);
            Sheet { title: "DAMAGE".into(), sub: "HIT OUTCOMES BY WHERE THE ROUND STRUCK".into(), lines: l }
        }
        _ => return None,
    })
}

// ---------------------------------------------------------------- screen

const LIST_W: f32 = 340.0;
const ROW_H: f32 = 28.0;
/// Source strip at the bottom of the sheet.
const STRIP_H: f32 = 74.0;
const VALUE_X: f32 = 300.0;

fn list_frame(screen: P2) -> Rect {
    Rect::new(40.0, 150.0, LIST_W, screen[1] - 150.0 - 100.0)
}

fn sheet_frame(screen: P2) -> Rect {
    let l = list_frame(screen);
    let x = l.x + l.w + 16.0;
    Rect::new(x, l.y, (screen[0] - 40.0 - x).max(300.0), l.h)
}

/// Scrolling area of the sheet (below the title, above the source strip).
fn body(screen: P2) -> Rect {
    let d = sheet_frame(screen);
    Rect::new(d.x, d.y + 70.0, d.w, d.h - 70.0 - STRIP_H)
}

fn content_h(s: &Sheet) -> f32 {
    s.lines.iter().map(Line::height).sum()
}

fn tag(o: &Origin) -> (&'static str, Rgba) {
    match o {
        Origin::Fact => ("", style::TEXT_OFF),
        Origin::Calc(_) => ("CALC", style::FRIENDLY),
        Origin::Stat { confidence, .. } => match confidence {
            Confidence::High => ("HIGH", style::TEXT),
            Confidence::Medium => ("MED", style::TEXT),
            Confidence::Low => ("LOW", style::AMBER),
            Confidence::Estimate => ("EST", style::TEXT_DIM),
        },
    }
}

impl Database {
    pub fn new(from_pause: bool) -> Self {
        Self { tab: Tab::Vehicles, sel: [0; 5], scroll: 0.0, from_pause }
    }

    /// Open on a vehicle (the game's selected-vehicle card).
    pub fn vehicle(ds: &DataSet, id: &str, from_pause: bool) -> Self {
        let mut d = Self::new(from_pause);
        d.sel[0] = ds.vehicles.iter().position(|v| v.id == id).unwrap_or(0);
        d
    }

    fn sel(&self) -> usize {
        self.sel[self.tab.index()]
    }

    pub fn buttons(&self, ds: &DataSet, screen: P2) -> Vec<Button<DbAction>> {
        let mut out = Vec::new();
        let mut x = 40.0;
        for (k, t) in Tab::ALL.into_iter().enumerate() {
            let w = hud::text_w(t.label(), style::FONT_S) + 36.0;
            let key = ["1", "2", "3", "4", "5"][k];
            out.push(
                Button::new(Rect::new(x, 108.0, w, 28.0), Look::Bar, t.label(), DbAction::Tab(t))
                    .key(key)
                    .active(t == self.tab),
            );
            x += w + 6.0;
        }
        let l = list_frame(screen);
        for (k, (label, value)) in entries(ds, self.tab).into_iter().enumerate() {
            let r = Rect::new(l.x + 8.0, l.y + 30.0 + k as f32 * (ROW_H + 4.0), l.w - 16.0, ROW_H);
            if r.y + r.h > l.y + l.h {
                break;
            }
            out.push(Button::new(r, Look::Row, label, DbAction::Pick(k)).value(value).active(k == self.sel()));
        }
        out.push(
            Button::new(Rect::new(40.0, screen[1] - 80.0, 200.0, 38.0), Look::Menu, "BACK", DbAction::Back).key("ESC"),
        );
        out
    }

    /// Returns true when the screen should close.
    pub fn update(&mut self, ds: &DataSet, inp: &Input) -> bool {
        let b = self.buttons(ds, inp.screen);
        let mut a = hud::clicked(&b, inp.mouse, inp.left_pressed);
        if inp.hit(Key::Escape) || inp.hit(Key::F(10)) {
            a = Some(DbAction::Back);
        }
        let k = self.tab.index();
        if inp.hit(Key::Right) {
            a = Some(DbAction::Tab(Tab::ALL[(k + 1) % Tab::ALL.len()]));
        }
        if inp.hit(Key::Left) {
            a = Some(DbAction::Tab(Tab::ALL[(k + Tab::ALL.len() - 1) % Tab::ALL.len()]));
        }
        for (n, t) in Tab::ALL.into_iter().enumerate() {
            if inp.hit(Key::Num(n as u8 + 1)) {
                a = Some(DbAction::Tab(t));
            }
        }
        let n = entries(ds, self.tab).len();
        if n > 0 {
            if inp.hit(Key::Down) {
                a = Some(DbAction::Pick((self.sel() + 1) % n));
            }
            if inp.hit(Key::Up) {
                a = Some(DbAction::Pick((self.sel() + n - 1) % n));
            }
        }
        match a {
            Some(DbAction::Back) => return true,
            Some(DbAction::Tab(t)) if t != self.tab => {
                self.tab = t;
                self.scroll = 0.0;
            }
            Some(DbAction::Pick(p)) if p != self.sel() => {
                self.sel[self.tab.index()] = p;
                self.scroll = 0.0;
            }
            _ => {}
        }
        let area = body(inp.screen);
        let mut dy = 0.0;
        if area.contains(inp.mouse) {
            dy -= inp.wheel * 60.0;
        }
        if inp.hit(Key::PageDown) {
            dy += area.h - 40.0;
        }
        if inp.hit(Key::PageUp) {
            dy -= area.h - 40.0;
        }
        let max = sheet(ds, self.tab, self.sel()).map_or(0.0, |s| (content_h(&s) - area.h + 10.0).max(0.0));
        self.scroll = (self.scroll + dy).clamp(0.0, max);
        false
    }

    pub fn draw(&self, f: &mut Frame, bd: &Backdrop, ds: &DataSet, inp: &Input, over_map: bool) {
        let (screen, mouse) = (inp.screen, inp.mouse);
        if over_map {
            f.rect(0.0, 0.0, screen[0], screen[1], Rgba(6, 9, 10, 236));
        } else {
            bd.draw(f, screen);
        }
        menu::heading(f, 40.0, 40.0, "DATABASE", "THE DATA THE SIMULATION RUNS ON. HOVER A VALUE FOR ITS SOURCE.");
        let l = list_frame(screen);
        hud::panel(f, l);
        f.text(self.tab.label(), l.x + 12.0, l.y + 10.0, style::FONT, style::TEXT_DIM);
        let d = sheet_frame(screen);
        hud::panel(f, d);
        let b = self.buttons(ds, screen);
        menu::draw_buttons(f, &b, mouse, screen);
        f.text(
            "LEFT/RIGHT TAB   UP/DOWN ENTRY   WHEEL / PGUP / PGDN SCROLL",
            260.0,
            screen[1] - 68.0,
            style::FONT_S,
            style::TEXT_DIM,
        );
        let Some(s) = sheet(ds, self.tab, self.sel()) else { return };
        let (x, w) = (d.x + 20.0, d.w - 40.0);
        f.text(&s.title, x, d.y + 16.0, style::FONT_L, style::AMBER);
        f.text(&s.sub, x, d.y + 44.0, style::FONT_S, style::TEXT_DIM);
        let area = body(screen);
        f.line([x, area.y - 4.0], [x + w, area.y - 4.0], 1.0, style::PANEL_EDGE);
        let mut y = area.y + 4.0 - self.scroll;
        let mut hover: Option<&Line> = None;
        for line in &s.lines {
            let h = line.height();
            let visible = y >= area.y - 2.0 && y + h <= area.y + area.h + 2.0;
            if visible {
                let row = Rect::new(d.x + 8.0, y - 2.0, d.w - 16.0, h);
                match line {
                    Line::Section(t) => {
                        f.text(t, x, y + 14.0, style::FONT, style::AMBER);
                        f.line([x, y + 28.0], [x + w, y + 28.0], 1.0, style::PANEL_EDGE);
                    }
                    Line::Row { label, value, origin } => {
                        if row.contains(mouse) {
                            f.rect(row.x, row.y, row.w, row.h, style::BUTTON_HOVER);
                            hover = Some(line);
                        }
                        f.text(label, x, y, style::FONT_S, style::TEXT_DIM);
                        f.text(value, x + VALUE_X, y, style::FONT_S, style::TEXT);
                        let (t, c) = tag(origin);
                        let tw = f.text_width(t, style::FONT);
                        f.text(t, x + w - tw, y + 1.0, style::FONT, c);
                    }
                    Line::Text(t) => f.text(t, x, y, style::FONT_S, style::TEXT),
                    Line::Swatch(rgb) => {
                        f.text("MAP COLOUR", x, y + 4.0, style::FONT_S, style::TEXT_DIM);
                        f.rect(x + VALUE_X, y, 80.0, 18.0, Rgba(rgb[0], rgb[1], rgb[2], 255));
                        f.rect_lines(x + VALUE_X, y, 80.0, 18.0, style::PANEL_EDGE);
                        let hex = format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
                        f.text(&hex, x + VALUE_X + 92.0, y + 4.0, style::FONT_S, style::TEXT);
                    }
                    Line::Formations(fs) => draw_formations(f, x, y, w, fs),
                }
            }
            y += h;
        }
        if content_h(&s) > area.h {
            // Scroll position.
            let total = content_h(&s);
            let bar_h = (area.h * area.h / total).max(20.0);
            let by = area.y + (area.h - bar_h) * (self.scroll / (total - area.h + 10.0)).min(1.0);
            f.rect(d.x + d.w - 5.0, by, 3.0, bar_h, style::PANEL_EDGE);
        }
        // Source strip.
        let sy = d.y + d.h - STRIP_H;
        f.line([x, sy], [x + w, sy], 1.0, style::PANEL_EDGE);
        let (head, body_text) = match hover {
            Some(Line::Row { label, origin: Origin::Stat { confidence, source, notes }, .. }) => (
                format!("{label} · CONFIDENCE {}", format!("{confidence:?}").to_uppercase()),
                format!("SOURCE: {source}{}", notes.as_ref().map_or(String::new(), |n| format!(". NOTE: {n}"))),
            ),
            Some(Line::Row { label, origin: Origin::Calc(formula), .. }) => {
                (format!("{label} · DERIVED"), format!("FORMULA: {formula}"))
            }
            Some(Line::Row { label, .. }) => (label.clone(), "AS GIVEN IN THE DATA FILE.".into()),
            _ => (
                "SOURCES".into(),
                "EVERY NUMBER CARRIES A PUBLIC SOURCE AND A CONFIDENCE: HIGH, MED, LOW OR EST (ESTIMATE). CALC VALUES ARE DERIVED WITH THE SIMULATION'S OWN FORMULAS.".into(),
            ),
        };
        f.text(&head, x, sy + 10.0, style::FONT, style::TEXT_DIM);
        for (n, t) in menu::wrap(f, &body_text.to_uppercase(), style::FONT_S, w).iter().take(2).enumerate() {
            f.text(t, x, sy + 28.0 + n as f32 * 18.0, style::FONT_S, style::TEXT);
        }
    }
}

/// Small slot diagrams, leader ringed, direction of travel up.
fn draw_formations(f: &mut Frame, x: f32, y: f32, w: f32, fs: &[(String, Vec<(f64, f64)>)]) {
    let n = fs.len().max(1) as f32;
    let cell = (w / n).min(140.0);
    for (k, (name, slots)) in fs.iter().enumerate() {
        let cx = x + k as f32 * cell + cell / 2.0;
        let cy = y + 50.0;
        let ext = slots.iter().fold(1.0f64, |e, (r, fw)| e.max(r.abs()).max(fw.abs()));
        let s = 34.0 / ext as f32;
        f.rect_lines(cx - cell / 2.0 + 4.0, y + 4.0, cell - 8.0, 92.0, style::PANEL_EDGE);
        f.line([cx, y + 10.0], [cx - 4.0, y + 16.0], 1.0, style::TEXT_OFF);
        f.line([cx, y + 10.0], [cx + 4.0, y + 16.0], 1.0, style::TEXT_OFF);
        for (i, (r, fw)) in slots.iter().enumerate() {
            let p = [cx + *r as f32 * s, cy + 10.0 - *fw as f32 * s];
            f.circle(p, 3.0, style::FRIENDLY);
            if i == 0 {
                f.circle_lines(p, 6.0, style::SELECT);
            }
        }
        let tw = f.text_width(name, style::FONT);
        f.text(name, cx - tw / 2.0, y + 102.0, style::FONT, style::TEXT_DIM);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ds() -> DataSet {
        DataSet::load(&std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")).unwrap()
    }

    #[test]
    fn every_entry_has_a_sheet_with_sourced_rows() {
        let ds = ds();
        for tab in Tab::ALL {
            let n = entries(&ds, tab).len();
            assert!(n > 0, "{tab:?} is empty");
            for k in 0..n {
                let s = sheet(&ds, tab, k).unwrap_or_else(|| panic!("{tab:?} {k}"));
                assert!(!s.lines.is_empty());
                for l in &s.lines {
                    if let Line::Row { origin: Origin::Stat { source, .. }, label, .. } = l {
                        assert!(!source.is_empty(), "{tab:?} {}: {label} has no source", s.title);
                    }
                }
            }
            assert!(sheet(&ds, tab, n).is_none());
        }
    }

    #[test]
    fn defeat_range_is_monotonic_in_armour() {
        let ds = ds();
        let a = &ds.ammo[0];
        let thin = defeat_range(&ds, a, 50.0);
        assert_eq!(thin, Some(5000.0), "50 mm is defeated at any range");
        assert_eq!(defeat_range(&ds, a, 5000.0), None);
        let mid = gunnery::penetration_mm(&ds.penetration, a, gunnery::velocity_at(a, 2000.0));
        let r = defeat_range(&ds, a, mid).unwrap();
        assert!((r - 2000.0).abs() <= 50.0, "{r}");
    }
}
