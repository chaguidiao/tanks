//! DATABASE screen (DESIGN.md §10.4): the vehicle, ammunition, terrain, doctrine and combat-model
//! data exactly as the simulation loads it from `data/`, with each value's source and
//! confidence, plus a few values derived with the sim's own formulas (marked CALC).
//!
//! Sheets are plain data built from the `DataSet` (testable without a window); drawing lays
//! them out as a scrolling table. Opened from the main menu, the screen menu, or the game's
//! selected-vehicle card (I).

use sim::gunnery;
use sim_data::{
    AmmoDef, Confidence, DataSet, DoctrinePack, LandCoverDef, PostureSignature, ServiceDates, SmallArmDef, SoilDef,
    SquadDef, Stat, VehicleDef,
};

use crate::hud::{self, Button, Look, Rect};
use crate::menu::{self, Backdrop};
use crate::render::{Frame, Input, Key, P2, Rgba};
use crate::style;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Vehicles,
    Infantry,
    Ammunition,
    Terrain,
    Doctrine,
    Combat,
}

impl Tab {
    pub const ALL: [Tab; 6] = [Tab::Vehicles, Tab::Infantry, Tab::Ammunition, Tab::Terrain, Tab::Doctrine, Tab::Combat];

    fn label(self) -> &'static str {
        match self {
            Tab::Vehicles => "VEHICLES",
            Tab::Infantry => "INFANTRY",
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
    Year,
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
    sel: [usize; 6],
    scroll: f32,
    /// Show only what is in service in this year (vehicles, ammunition, batteries).
    pub year: Option<i32>,
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

/// A sight's search field and the share of its in-view detection rate it keeps while sweeping the
/// search sector (B24 D2).
fn fov_line(ds: &DataSet, s: &sim_data::SensorDef) -> Option<Line> {
    let sector = ds.detection.search_sector_deg.get();
    s.field_of_view_deg
        .as_ref()
        .map(|f| stat("  FIELD OF VIEW", f, |x| format!("{x:.1}° · SEARCH RATE × {:.2}", (x / sector).min(1.0))))
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

/// Entries of a tab: (index into the data, label, value) rows of the list. With a `year` the
/// era-dated tabs list only what was in service then.
pub fn entries(ds: &DataSet, tab: Tab, year: Option<i32>) -> Vec<(usize, String, String)> {
    let live = |d: &ServiceDates| year.is_none_or(|y| d.contains(y));
    match tab {
        Tab::Vehicles => ds
            .vehicles
            .iter()
            .enumerate()
            .filter(|(_, v)| live(&v.in_service))
            .map(|(k, v)| (k, v.name.clone(), years(&v.in_service)))
            .collect(),
        Tab::Infantry => ds
            .squads
            .iter()
            .enumerate()
            .filter(|(_, s)| live(&s.in_service))
            .map(|(k, s)| (k, s.name.clone(), years(&s.in_service)))
            .chain(std::iter::once((ds.squads.len(), "INFANTRY COMBAT MODEL".to_owned(), String::new())))
            .collect(),
        Tab::Ammunition => ds
            .ammo
            .iter()
            .enumerate()
            .filter(|(_, a)| live(&a.in_service))
            .map(|(k, a)| (k, a.name.clone(), years(&a.in_service)))
            .collect(),
        Tab::Terrain => ds
            .terrain
            .landcover
            .iter()
            .map(|l| (upper(&l.name), "LAND COVER".to_owned()))
            .chain(ds.terrain.soils.iter().map(|s| (upper(&s.name), "SOIL".to_owned())))
            .enumerate()
            .map(|(k, (a, b))| (k, a, b))
            .collect(),
        Tab::Doctrine => {
            ds.doctrine.iter().enumerate().map(|(k, d)| (k, d.name.to_uppercase(), d.id.to_uppercase())).collect()
        }
        Tab::Combat => {
            let fixed = ["DETECTION", "PENETRATION", "DAMAGE", "HE EFFECTS", "SUPPRESSION"];
            let n = fixed.len();
            fixed
                .iter()
                .enumerate()
                .map(|(k, s)| (k, (*s).to_owned(), String::new()))
                .chain(
                    ds.fires
                        .batteries
                        .iter()
                        .enumerate()
                        .filter(|(_, b)| live(&b.in_service))
                        .map(|(k, b)| (n + k, b.name.clone(), years(&b.in_service))),
                )
                .collect()
        }
    }
}

/// Years the filter steps through: ALL, then every five years.
const YEAR_STEPS: [i32; 13] = [1970, 1975, 1980, 1985, 1990, 1995, 2000, 2005, 2010, 2015, 2020, 2025, 2030];

fn next_year(y: Option<i32>) -> Option<i32> {
    match y {
        None => Some(YEAR_STEPS[0]),
        Some(y) => YEAR_STEPS.iter().copied().find(|s| *s > y),
    }
}

pub fn sheet(ds: &DataSet, tab: Tab, k: usize) -> Option<Sheet> {
    Some(match tab {
        Tab::Vehicles => vehicle_sheet(ds, ds.vehicles.get(k)?),
        Tab::Infantry if k == ds.squads.len() => infantry_model_sheet(ds),
        Tab::Infantry => squad_sheet(ds, ds.squads.get(k)?),
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
    ];
    // Supply trucks: cargo instead of weapons (M8c tailgate resupply).
    let armed = !a.load.is_empty();
    if let Some(c) = &v.cargo {
        l.extend([
            section("CARGO"),
            stat("PAYLOAD", &c.payload_kg, |x| format!("{:.1} T", x / 1000.0)),
            stat("FUEL", &c.fuel_l, |x| format!("{x:.0} L")),
            fact("MAIN-GUN ROUNDS", c.main_gun_rounds.to_string()),
            fact("AUTOCANNON ROUNDS", c.autocannon_rounds.to_string()),
            fact("MISSILES", c.missiles.to_string()),
            fact("SMOKE GRENADE SALVOS", c.smoke_salvos.to_string()),
            fact("FILLED WITH", "THE ROUNDS ITS SIDE'S VEHICLES CARRY, UNLESS THE SCENARIO LISTS THEM"),
        ]);
    }
    if armed {
        l.extend([
            section("ARMAMENT"),
            fact("MAIN GUN", a.gun.clone()),
            stat("ELEVATION", &v.gun.elevation_rad, deg),
            stat("DEPRESSION", &v.gun.depression_rad, deg),
        ]);
    }
    for ld in &a.load {
        let name = ds.ammo(&ld.ammo).map_or(ld.ammo.clone(), |x| x.name.clone());
        let when = ld.in_service.as_ref().map_or(String::new(), |d| format!(" ({})", years(d)));
        l.push(fact("LOAD", format!("{} × {name}{when}", ld.count)));
    }
    l.extend(
        armed
            .then(|| {
                [
                    fact("LOADING", if a.autoloader { "AUTOLOADER" } else { "HUMAN LOADER" }),
                    stat("RELOAD", &a.reload_s, secs),
                    stat("TURRET TRAVERSE", &a.turret_traverse_rad_s, |x| format!("{:.0} °/S", x.to_degrees())),
                    calc(
                        "FULL TRAVERSE",
                        secs(std::f64::consts::TAU / a.turret_traverse_rad_s.get()),
                        "360° / TRAVERSE RATE",
                    ),
                    stat("ENGAGEMENT RANGE (CREW)", &a.max_engagement_range_m, |x| format!("{x:.0} M")),
                    fact(
                        "AMMUNITION STOWAGE",
                        match v.ammo_stowage {
                            sim_data::AmmoStowage::Isolated => "ISOLATED (BLOW-OUT PANELS)",
                            sim_data::AmmoStowage::Exposed => "EXPOSED (HIGH COOK-OFF RISK)",
                        },
                    ),
                ]
            })
            .into_iter()
            .flatten(),
    );
    if let Some(c) = &v.coax {
        l.extend(small_arm_lines("COAX", c));
    }
    if let Some(t) = &v.troops {
        l.extend([
            section("TROOPS"),
            fact("SEATS", format!("{} MEN", t.capacity_men)),
            stat("DISMOUNT DRILL", &t.dismount_s, secs),
            stat("MOUNT DRILL", &t.mount_s, secs),
        ]);
    }
    l.extend(
        armed
            .then(|| {
                [
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
                ]
            })
            .into_iter()
            .flatten(),
    );
    l.push(section("SENSORS"));
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
        l.extend(fov_line(ds, s));
    }
    if v.smoke_launcher.is_some() || v.engine_smoke.is_some() {
        l.push(section("SMOKE"));
        if let Some(sl) = &v.smoke_launcher {
            l.push(fact("GRENADE LAUNCHERS", format!("{} SALVOS · {} CLOUDS EACH", sl.salvos, sl.clouds_per_salvo)));
            l.push(stat("SCREEN RANGE", &sl.range_m, |x| format!("{x:.0} M")));
            l.push(stat("SCREEN ARC", &sl.arc_deg, |x| format!("{x:.0} DEG")));
        }
        let ids = v.smoke_launcher.iter().flat_map(|s| &s.kinds).chain(v.engine_smoke.iter().flat_map(|e| &e.kinds));
        for k in ids.filter_map(|id| ds.smoke.kinds.iter().find(|k| &k.id == id)) {
            l.push(stat(&k.name, &k.tau_visible, |x| {
                format!(
                    "OD VISUAL {x:.1} · THERMAL {:.1} · R {:.0} M · {:.0} S · {}",
                    k.tau_thermal.value,
                    k.radius_m.value,
                    k.life_s.value,
                    years(&k.in_service)
                )
            }));
        }
    }
    l.extend([
        section("ARMOUR (RHA EQUIVALENT VS KE, NORMAL ATTACK)"),
        fact("CLASS", v.class.label()),
        stat("CE FACTOR (SHAPED-CHARGE PROTECTION)", &v.armour.ce_factor, |x| format!("× {x:.2}")),
        stat("TURRET FRONT", &v.armour.turret_front, mm),
        stat("TURRET SIDE", &v.armour.turret_side, mm),
        stat("TURRET REAR", &v.armour.turret_rear, mm),
        stat("HULL FRONT", &v.armour.hull_front, mm),
        stat("HULL SIDE", &v.armour.hull_side, mm),
        stat("HULL REAR", &v.armour.hull_rear, mm),
        stat("TOP (ROOF AND ENGINE DECK)", &v.armour.top, mm),
    ]);
    if let Some(e) = &v.armour.era {
        l.push(section("EXPLOSIVE REACTIVE ARMOUR (FRONT AND SIDES)"));
        l.push(fact("TILES", format!("{} · IN SERVICE {}", e.tiles, years(&e.in_service))));
        l.push(stat("COVERAGE", &e.coverage, |x| format!("{:.0} % OF HITS", x * 100.0)));
        l.push(stat("VS SHAPED CHARGE", &e.ce_defeat, |x| format!("{:.0} % PENETRATION REMOVED", x * 100.0)));
        l.push(stat("VS KINETIC", &e.ke_gain_mm, |x| format!("+{x:.0} MM RHA")));
    }
    let threats: Vec<&AmmoDef> = ds.ammo.iter().filter(|a| a.kind != sim_data::AmmoKind::Smoke).collect();
    if !threats.is_empty() {
        l.push(section("FRONTAL ARMOUR DEFEATED BY"));
        for t in threats {
            l.push(calc(&t.name, defeat_text(ds, t, v), DEFEAT_FORMULA));
        }
    }
    Sheet {
        title: v.name.clone(),
        sub: format!("MIRRORS {} · IN SERVICE {} · CREW {}", v.mirrors.to_uppercase(), years(&v.in_service), v.crew),
        lines: l,
    }
}

/// Farthest range the sheet scans (m); also the step is 50 m.
const SCAN_M: f64 = 5000.0;

/// How a target's front meets a round: the armour to beat (mm) and the share of the round's
/// penetration left after an ERA tile fires (1.0 without ERA).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Front {
    armour_mm: f64,
    pen_factor: f64,
}

/// Longest range (to the round's maximum range or 5 km, 50 m steps) at which `a` still penetrates
/// `front`; None if never.
fn defeat_range(ds: &DataSet, a: &AmmoDef, front: Front) -> Option<f64> {
    let steps = (scan_limit(a) / 50.0) as u32;
    (0..=steps).rev().map(|k| k as f64 * 50.0).find(|&r| {
        front.pen_factor * gunnery::penetration_mm(&ds.penetration, a, gunnery::velocity_at(a, r)) >= front.armour_mm
    })
}

/// The range the scan stops at: the round's maximum range if it has one, else 5 km.
fn scan_limit(a: &AmmoDef) -> f64 {
    a.max_range_m.as_ref().map_or(SCAN_M, |m| m.get().min(SCAN_M))
}

/// Kilometres with one decimal, two when that would round (3750 m reads 3.75, not 3.8).
fn km(m: f64) -> String {
    if (m % 100.0).abs() < 1e-6 { format!("{:.1}", m / 1000.0) } else { format!("{:.2}", m / 1000.0) }
}

fn range_text(r: Option<f64>, a: &AmmoDef) -> String {
    let limit = scan_limit(a);
    match r {
        None => "NO".into(),
        Some(r) if r >= (limit / 50.0).floor() * 50.0 => {
            if limit >= SCAN_M {
                "ANY RANGE".into()
            } else {
                format!("TO {} KM (MAX RANGE)", km(limit))
            }
        }
        Some(r) => format!("TO {} KM", km(r)),
    }
}

/// The front `mm` thick as `a` meets it at normal incidence: chemical rounds meet the CE-equivalent
/// thickness and, with ERA, lose the share a fired tile removes; kinetic rounds meet the extra
/// RHA a fired tile adds. This is the case with a tile still on the spot (the sim spends tiles).
fn front_vs(a: &AmmoDef, v: &VehicleDef, mm: f64) -> Front {
    let era = v.armour.era.as_ref();
    if a.kind.is_chemical() {
        let defeat = era.filter(|_| !a.tandem).map_or(0.0, |e| e.ce_defeat.get());
        Front { armour_mm: mm * v.armour.ce_factor.get(), pen_factor: 1.0 - defeat }
    } else {
        Front { armour_mm: mm + era.map_or(0.0, |e| e.ke_gain_mm.get()), pen_factor: 1.0 }
    }
}

fn defeat_text(ds: &DataSet, a: &AmmoDef, v: &VehicleDef) -> String {
    let at = |mm: f64| range_text(defeat_range(ds, a, front_vs(a, v, mm)), a);
    format!("TURRET FRONT {}  ·  HULL FRONT {}", at(v.armour.turret_front.get()), at(v.armour.hull_front.get()))
}

const DEFEAT_FORMULA: &str = "PENETRATION AT REMAINING VELOCITY ≥ ARMOUR, TO THE ROUND'S MAX RANGE (CHEMICAL ROUNDS: × CE FACTOR; ERA TILE ASSUMED TO FIRE, NO LOSS FOR TANDEM)";

/// Rows for one small arm or MG (burst model, plan M8B-INFANTRY §4).
fn small_arm_lines(head: &str, w: &SmallArmDef) -> Vec<Line> {
    let mut l = vec![section(&format!("{head}: {}", w.name))];
    if let Some(d) = &w.in_service {
        l.push(fact("IN SERVICE", years(d)));
    }
    l.extend([
        fact("WEAPONS", format!("{}", w.count)),
        stat("ROUNDS PER BURST", &w.rounds_per_burst, plain),
        stat("BURST INTERVAL", &w.burst_interval_s, secs),
        calc(
            "SUSTAINED RATE",
            format!("{:.0} RDS/MIN", w.rounds_per_burst.get() * 60.0 / w.burst_interval_s.get()),
            "ROUNDS PER BURST · 60 / INTERVAL",
        ),
        stat("MAX RANGE", &w.max_range_m, |x| format!("{x:.0} M")),
        stat("DISPERSION", &w.dispersion_mrad, mrad),
        fact("BASIC LOAD", format!("{} BURSTS PER WEAPON", w.bursts)),
    ]);
    l
}

/// Rows for one anti-armour weapon of a squad (plan M8B-AT-TEAMS §2.1).
fn anti_armour_lines(w: &sim_data::AntiArmourDef) -> Vec<Line> {
    let class = match w.class {
        sim_data::AntiArmourClass::Rocket => "ROCKET",
        sim_data::AntiArmourClass::Missile => "MISSILE",
    };
    let mut l = vec![section(&format!("ANTI-ARMOUR: {} ({class})", w.name))];
    if let Some(d) = &w.in_service {
        l.push(fact("IN SERVICE", years(d)));
    }
    let load: Vec<String> = w.load.iter().map(|x| format!("{} × {}", x.count, x.ammo.to_uppercase())).collect();
    l.extend([
        fact("LAUNCHERS", format!("{}", w.count)),
        fact("ROUNDS PER LAUNCHER", load.join(", ")),
        stat("AIM ERROR", &w.sight.aim_error_mrad, mrad),
        stat("RANGE ESTIMATION ERROR", &w.sight.estimated_range_error_fraction, pct),
        stat("BALLISTIC ERROR", &w.sight.ballistic_error_mrad, mrad),
        stat("SET-UP TIME", &w.setup_s, secs),
        stat("TIME BETWEEN SHOTS", &w.reload_s, secs),
        stat("LAUNCH SIGNATURE", &w.signature_factor, |x| format!("× {x:.1} FIRING CUE")),
    ]);
    l
}

fn posture_lines(head: &str, p: &PostureSignature) -> Vec<Line> {
    vec![
        section(head),
        stat("WIDTH SEEN", &p.width_m, m),
        stat("HEIGHT SEEN", &p.height_m, m),
        calc(
            "CRITICAL SIZE",
            m((p.width_m.get() * p.height_m.get()).sqrt()),
            "SQRT(WIDTH · HEIGHT) (JOHNSON CRITERIA INPUT)",
        ),
        stat("EXPOSED AREA PER MAN", &p.man_area_m2, |x| format!("{x:.2} M²")),
        stat("EYE HEIGHT", &p.eye_height_m, m),
    ]
}

fn squad_sheet(ds: &DataSet, s: &SquadDef) -> Sheet {
    let mut l = vec![
        section("GENERAL"),
        fact("MIRRORS", s.mirrors.to_uppercase()),
        fact("IN SERVICE", years(&s.in_service)),
        fact("MEN", format!("{}", s.men)),
        stat("DIG-IN TIME", &s.dig_in_s, |x| format!("{:.0} MIN", x / 60.0)),
    ];
    for w in &s.weapons {
        l.extend(small_arm_lines("WEAPON", w));
    }
    for w in &s.anti_armour {
        l.extend(anti_armour_lines(w));
    }
    l.push(section("SENSORS"));
    for se in &s.sensors {
        l.push(stat(&format!("{} ({})", se.name, years(&se.in_service)), &se.cycles_per_mrad, |x| {
            format!("{x:.1} CYC/MRAD")
        }));
        l.extend(fov_line(ds, se));
    }
    let sig = &s.signature;
    l.extend(posture_lines("SIGNATURE: MOVING", &sig.moving));
    l.extend(posture_lines("SIGNATURE: HALTED", &sig.halted));
    l.extend(posture_lines("SIGNATURE: DUG IN", &sig.dug_in));
    l.extend(posture_lines("SIGNATURE: IN A BUILDING", &sig.in_building));
    let carriers: Vec<String> = ds
        .vehicles
        .iter()
        .filter(|v| v.troops.as_ref().is_some_and(|t| t.capacity_men >= s.men))
        .map(|v| v.name.clone())
        .collect();
    l.push(section("CARRIED BY"));
    l.push(Line::Text(if carriers.is_empty() { "NONE".into() } else { carriers.join(", ") }));
    Sheet { title: s.name.clone(), sub: "SQUAD (ONE ELEMENT WITH A STRENGTH, DESIGN.MD §14 #29)".into(), lines: l }
}

fn infantry_model_sheet(ds: &DataSet) -> Sheet {
    let m = &ds.infantry;
    let mut l = vec![
        section("SMALL ARMS (ONE DRAW PER BURST)"),
        stat("LETHALITY PER HIT", &m.lethality_per_hit, pct),
        stat("MAX HIT PER ROUND", &m.max_round_hit, pct),
        calc(
            "PRONE MAN, PK BURST AT 300 M",
            format!("{:.2} CASUALTIES", 8.0 * m.round_hit(0.15, 5.0, 300.0) * m.lethality_per_hit.get()),
            "8 RDS · AREA / (2π (σ R)²) · LETHALITY",
        ),
        stat("SUPPRESSION PER 10 RDS", &m.suppression_per_burst, plain),
        stat("SUPPRESSION ON VEHICLES", &m.vehicle_suppression_per_burst, plain),
        stat("PINNED ABOVE", &m.pinned_above, plain),
        stat("FIRE AT VEHICLES INSIDE", &m.vehicle_engage_m, |x| format!("{x:.0} M")),
        section("HE"),
        stat("SQUAD SPREAD RADIUS", &m.spread_radius_m, |x| format!("{x:.0} M")),
        stat("EXPOSURE MOVING", &m.he_exposure.moving, plain),
        stat("EXPOSURE HALTED", &m.he_exposure.halted, plain),
        stat("EXPOSURE DUG IN", &m.he_exposure.dug_in, plain),
        section("DETECTION"),
        stat("THERMAL CROSSOVER SHARE", &m.thermal_crossover_share, plain),
        stat("OPTICAL CONTRAST", &m.optical_contrast, |x| format!("× {x:.2}")),
        stat("CAMOUFLAGE HALTED", &m.camouflage.halted, |x| format!("× {x:.2}")),
        stat("CAMOUFLAGE DUG IN", &m.camouflage.dug_in, |x| format!("× {x:.2}")),
        stat("CAMOUFLAGE IN BUILDING", &m.camouflage_in_building, |x| format!("× {x:.2}")),
        stat("TALL CROPS VISIBLE", &m.tall_crops.visible_fraction, pct),
        fact("TALL CROP MONTHS", m.tall_crops.months.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(" ")),
        section("MOUNTED SQUADS"),
        stat("LOST ON CATASTROPHIC KILL", &m.mounted_loss.catastrophic, pct),
        stat("LOST ON PENETRATION", &m.mounted_loss.penetration, pct),
        section("BUILDINGS"),
    ];
    for b in &ds.terrain.buildings {
        let n = format!("{:?}", b.class).to_uppercase();
        l.extend([
            stat(&format!("{n} WALL"), &b.wall_thickness_m, |x| format!("{x:.2} M")),
            stat(&format!("{n} VS SMALL ARMS"), &b.protection.small_arms, pct),
            stat(&format!("{n} VS FRAGMENTS"), &b.protection.fragments, pct),
            stat(&format!("{n} VS BLAST"), &b.protection.blast, pct),
            stat(&format!("{n} SQUADS PER 100 M²"), &b.squads_per_100m2, plain),
            stat(&format!("{n} MOST SQUADS"), &b.max_squads, |x| format!("{x:.0}")),
        ]);
    }
    Sheet { title: "INFANTRY COMBAT MODEL".into(), sub: "DATA/COMBAT/INFANTRY.RON".into(), lines: l }
}

fn ammo_sheet(ds: &DataSet, a: &AmmoDef) -> Sheet {
    let mut l = vec![
        section("PROJECTILE"),
        stat("MUZZLE VELOCITY", &a.muzzle_velocity_mps, |x| format!("{x:.0} M/S")),
        stat("VELOCITY LOSS", &a.velocity_loss_mps_per_km, |x| format!("{x:.0} M/S PER KM")),
        stat("DISPERSION", &a.dispersion_mrad, mrad),
    ];
    if let (Some(len), Some(dia), Some(rho)) = (&a.rod_length_mm, &a.rod_diameter_mm, &a.rod_density_g_cm3) {
        l.push(stat("ROD LENGTH", len, mm));
        l.push(stat("ROD DIAMETER", dia, mm));
        l.push(calc("LENGTH / DIAMETER", format!("{:.0}", len.get() / dia.get()), "L / D"));
        l.push(stat("ROD DENSITY", rho, |x| format!("{x:.1} G/CM³")));
    }
    if let Some(p) = &a.flat_penetration_mm {
        l.push(stat("PENETRATION (RHA, NORMAL)", p, mm));
    }
    if let Some(r) = &a.reload_s {
        l.push(stat("CYCLE TIME", r, |x| format!("{x:.1} S")));
    }
    if let Some(r) = &a.max_range_m {
        l.push(stat("MAXIMUM RANGE", r, |x| format!("{x:.0} M")));
    }
    if let Some(r) = &a.min_range_m {
        l.push(stat("MINIMUM RANGE", r, |x| format!("{x:.0} M")));
    }
    if a.guided {
        l.push(fact(
            "GUIDANCE",
            if a.fire_and_forget { "FIRE AND FORGET" } else { "COMMAND (GUNNER TRACKS TO IMPACT)" },
        ));
    }
    if a.tandem {
        l.push(fact("WARHEAD", "TANDEM (PRECURSOR FIRES REACTIVE ARMOUR)"));
    }
    if let Some(t) = &a.top_attack_deg {
        l.push(stat("TOP ATTACK DIVE", t, |x| format!("{x:.0}° BELOW HORIZONTAL")));
    }
    if let Some(b) = &a.behind_armour {
        l.push(stat("BEHIND-ARMOUR EFFECT", b, |x| format!("× {x:.2} KILL OUTCOMES")));
    }
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
            "VELOCITY: LINEAR LOSS; DROP: G·T²/2; PENETRATION: LANZ–ODERMATT (APFSDS), FLAT (OTHERS)",
        ));
    }
    l.push(section("DEFEATS FRONTAL ARMOUR OF"));
    for v in ds.vehicles.iter().filter(|_| a.kind != sim_data::AmmoKind::Smoke) {
        l.push(calc(&v.name, defeat_text(ds, a, v), DEFEAT_FORMULA));
    }
    l.push(section("CARRIED BY"));
    for v in ds.vehicles.iter().filter(|v| v.armament.load.iter().any(|x| x.ammo == a.id)) {
        for x in v.armament.load.iter().filter(|x| x.ammo == a.id) {
            let when = x.in_service.as_ref().map_or(String::new(), |d| format!(" ({})", years(d)));
            l.push(fact(&v.name, format!("{} ROUNDS · {}{when}", x.count, v.armament.gun)));
        }
    }
    for s in &ds.squads {
        for w in s.anti_armour.iter().filter(|w| w.load.iter().any(|x| x.ammo == a.id)) {
            let n: u32 = w.load.iter().filter(|x| x.ammo == a.id).map(|x| x.count * w.count as u32).sum();
            let when = w.in_service.as_ref().map_or(String::new(), |d| format!(" ({})", years(d)));
            l.push(fact(&s.name, format!("{n} ROUNDS · {}{when}", w.name)));
        }
    }
    let kind = a.kind.label();
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
    let mut l = vec![
        section("MOVEMENT"),
        fact("DEFAULT FORMATION", formation(d.default_formation)),
        stat("VEHICLE INTERVAL", &d.spacing_m, |x| format!("{x:.0} M")),
        stat("BOUND LENGTH", &d.bound_length_m, |x| format!("{x:.0} M")),
        stat("OVERWATCH DISTANCE", &d.overwatch_distance_m, |x| format!("{x:.0} M")),
    ];
    for (i, sp) in d.speed_limits_mps.iter().enumerate() {
        l.push(stat(&format!("ORDERED SPEED {}", i + 1), sp, |x| format!("{:.0} KM/H", x * 3.6)));
    }
    l.extend([
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
    ]);
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
            let curve: Vec<Line> = d
                .light_curve
                .iter()
                .map(|p| stat(&format!("SUN AT {:+.0}°", p.sun_elevation_deg), &p.factor, |x| format!("× {x:.3}")))
                .collect();
            let mut lines = vec![
                section("CYCLES FOR 50 % PROBABILITY (N50)"),
                stat("DETECT", &d.n50_detect, plain),
                stat("RECOGNISE (TANK VS OTHER)", &d.n50_recognise, plain),
                stat("IDENTIFY (WHICH TANK)", &d.n50_identify, plain),
                section("SEARCH"),
                stat("ACQUIRE TIME CONSTANT", &d.acquire_tau_s, secs),
                stat("TARGET MOVING", &d.moving_target_factor, |x| format!("× {x:.2}")),
                stat("TARGET FIRING", &d.firing_target_factor, |x| format!("× {x:.2}")),
                stat("OBSERVER MOVING", &d.observer_moving_factor, |x| format!("× {x:.2}")),
                stat("SEARCH SECTOR", &d.search_sector_deg, |x| {
                    format!("{x:.0}° (NARROW SIGHTS FIND NEW TARGETS AT FIELD / SECTOR)")
                }),
                section("OPTICAL SIGHTS BY LIGHT"),
            ];
            lines.extend(curve);
            lines.extend([
                stat("MOON GAIN", &d.moon_gain, |x| format!("× {x:.1} AT FULL MOON")),
                stat("XOVER CENTRE", &d.thermal_crossover.centre_elevation_deg, |x| format!("{x:.0}° SUN")),
                stat("XOVER WIDTH", &d.thermal_crossover.width_deg, |x| format!("{x:.0}°")),
                stat("XOVER DEPTH", &d.thermal_crossover.depth, |x| format!("-{:.0} %", x * 100.0)),
                section("LIMITS"),
                stat("DETECTION CHECKS OUT TO", &d.max_range_m, |x| format!("{x:.0} M")),
                stat("TRACK DROPPED AFTER", &d.track_timeout_s, secs),
            ]);
            Sheet {
                title: "DETECTION".into(),
                sub: "JOHNSON CRITERIA / TTP CYCLES ACROSS THE TARGET, ACQUIRE SEARCH TIME".into(),
                lines,
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
        3 => {
            let e = &ds.fires.effects;
            let mut l = vec![
                Line::Text("OUTCOME PROBABILITY PER HE SHELL BY VEHICLE CLASS. ESTIMATES, NOT TEST DATA.".into()),
                stat("DIRECT ZONE RADIUS", &e.direct_radius_m, |x| format!("{x:.0} M")),
                stat("NEAR ZONE RADIUS", &e.near_radius_m, |x| format!("{x:.0} M")),
            ];
            for c in &e.classes {
                for (zone, o) in [("DIRECT", &c.direct), ("NEAR", &c.near)] {
                    l.extend([
                        section(&format!("{} — {zone}", upper(&format!("{:?}", c.class)))),
                        stat("CATASTROPHIC KILL", &o.catastrophic, pct),
                        stat("FIREPOWER KILL", &o.firepower, pct),
                        stat("MOBILITY KILL", &o.mobility, pct),
                        stat("CREW CASUALTIES", &o.crew, pct),
                    ]);
                }
                l.push(stat("SIGHTS DAMAGED (NEAR)", &c.near_sights, pct));
            }
            Sheet { title: "HE EFFECTS".into(), sub: "ARTILLERY HE AGAINST VEHICLES".into(), lines: l }
        }
        4 => {
            let s = &ds.fires.suppression;
            Sheet {
                title: "SUPPRESSION".into(),
                sub: "CREW REACTION TO NEAR MISSES AND BURSTS (0 TO 100 %)".into(),
                lines: vec![
                    section("SOURCES"),
                    stat("BURST RADIUS", &s.burst_radius_m, |x| format!("{x:.0} M")),
                    stat("BURST AT CENTRE", &s.burst_peak, pct),
                    stat("DIRECT-FIRE MISS", &s.near_miss, pct),
                    stat("NON-PENETRATING HIT", &s.glancing_hit, pct),
                    stat("DECAY", &s.decay_per_s, |x| format!("{:.1} % / S", x * 100.0)),
                    section("EFFECTS AT FULL SUPPRESSION"),
                    stat("DETECTION LOST", &s.sensor_penalty, pct),
                    stat("GUNNERY SLOWED BY", &s.slow_gain, pct),
                    section("PLATOON DRILL: REACT TO INDIRECT FIRE"),
                    stat("BURST WITHIN", &s.react_radius_m, |x| format!("{x:.0} M")),
                    stat("IN THE LAST", &s.react_window_s, secs),
                    stat("REPORT COOLDOWN", &s.react_cooldown_s, secs),
                ],
            }
        }
        k if k >= 5 => {
            let b = ds.fires.batteries.get(k - 5)?;
            Sheet {
                title: b.name.clone(),
                sub: format!("MIRRORS {}", b.mirrors.to_uppercase()),
                lines: vec![
                    fact("IN SERVICE", years(&b.in_service)),
                    fact("TUBES", b.tubes.to_string()),
                    fact("HE / SMOKE ROUNDS", format!("{} / {}", b.he_rounds, b.smoke_rounds)),
                    section("RANGE"),
                    stat("MINIMUM", &b.min_range_m, |x| format!("{x:.0} M")),
                    stat("MAXIMUM", &b.max_range_m, |x| format!("{x:.0} M")),
                    section("TIMING"),
                    stat("FDC DELAY (CALL TO SHOT)", &b.fdc_delay_s, secs),
                    stat("VOLLEY INTERVAL", &b.volley_interval_s, secs),
                    stat("TIME OF FLIGHT, BASE", &b.tof_base_s, secs),
                    stat("TIME OF FLIGHT, PER KM", &b.tof_per_km_s, secs),
                    section("ACCURACY (UNADJUSTED)"),
                    stat("RANGE ERROR, 1 SD", &b.range_error_frac, |x| format!("{:.2} % OF RANGE", x * 100.0)),
                    stat("DEFLECTION ERROR, 1 SD", &b.deflection_error_mrad, mrad),
                    calc(
                        "RANGE ERROR AT 10 KM",
                        format!("{:.0} M", b.range_error_frac.value * 10_000.0),
                        "range_error_frac × range",
                    ),
                    calc(
                        "TIME OF FLIGHT AT 10 KM",
                        format!("{:.0} S", b.tof_base_s.value + b.tof_per_km_s.value * 10.0),
                        "tof_base + tof_per_km × range / 1000",
                    ),
                ]
                .into_iter()
                .chain(b.resupply.iter().flat_map(|r| {
                    [
                        section("RESUPPLY"),
                        fact("BASIC LOADS IN THE TRAINS", r.loads.to_string()),
                        stat("STARTS BELOW", &r.threshold_frac, |x| format!("{:.0} % OF THE HE LOAD", x * 100.0)),
                        stat("TIME TO RESTOCK", &r.resupply_s, secs),
                    ]
                }))
                .chain(b.displace.iter().flat_map(|m| {
                    [
                        section("SURVIVABILITY MOVES"),
                        fact("DISPLACES AFTER", format!("{} MISSIONS", m.after_missions)),
                        stat("OUT OF ACTION", &m.displace_s, secs),
                    ]
                }))
                .collect(),
            }
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
        Self { tab: Tab::Vehicles, sel: [0; 6], scroll: 0.0, year: None, from_pause }
    }

    /// Open on a vehicle or squad type (the game's selected-unit card).
    pub fn vehicle(ds: &DataSet, id: &str, from_pause: bool) -> Self {
        let mut d = Self::new(from_pause);
        if let Some(k) = ds.squads.iter().position(|s| s.id == id) {
            d.tab = Tab::Infantry;
            d.sel[Tab::Infantry.index()] = k;
        } else {
            d.sel[0] = ds.vehicles.iter().position(|v| v.id == id).unwrap_or(0);
        }
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
            let key = ["1", "2", "3", "4", "5", "6"][k];
            out.push(
                Button::new(Rect::new(x, 108.0, w, 28.0), Look::Bar, t.label(), DbAction::Tab(t))
                    .key(key)
                    .active(t == self.tab),
            );
            x += w + 6.0;
        }
        let l = list_frame(screen);
        for (pos, (k, label, value)) in entries(ds, self.tab, self.year).into_iter().enumerate() {
            let r = Rect::new(l.x + 8.0, l.y + 30.0 + pos as f32 * (ROW_H + 4.0), l.w - 16.0, ROW_H);
            if r.y + r.h > l.y + l.h {
                break;
            }
            out.push(Button::new(r, Look::Row, label, DbAction::Pick(k)).value(value).active(k == self.sel()));
        }
        let year = self.year.map_or("ALL".to_owned(), |y| y.to_string());
        out.push(
            Button::new(
                Rect::new(x + 12.0, 108.0, 190.0, 28.0),
                Look::Bar,
                format!("IN SERVICE {year}"),
                DbAction::Year,
            )
            .key("Y")
            .tip("Show only what is in service in this year. Click or Y steps by five years."),
        );
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
        if inp.hit(Key::Char('y')) {
            a = Some(DbAction::Year);
        }
        let list = entries(ds, self.tab, self.year);
        let n = list.len();
        if n > 0 {
            let at = list.iter().position(|e| e.0 == self.sel()).unwrap_or(0);
            if inp.hit(Key::Down) {
                a = Some(DbAction::Pick(list[(at + 1) % n].0));
            }
            if inp.hit(Key::Up) {
                a = Some(DbAction::Pick(list[(at + n - 1) % n].0));
            }
        }
        match a {
            Some(DbAction::Back) => return true,
            Some(DbAction::Tab(t)) if t != self.tab => {
                self.tab = t;
                self.scroll = 0.0;
            }
            Some(DbAction::Year) => {
                self.year = next_year(self.year);
                let list = entries(ds, self.tab, self.year);
                if let Some(e) = list.first()
                    && !list.iter().any(|x| x.0 == self.sel())
                {
                    self.sel[self.tab.index()] = e.0;
                    self.scroll = 0.0;
                }
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
            let n = entries(&ds, tab, None).len();
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
    fn year_filter_keeps_only_what_is_in_service() {
        let ds = ds();
        let all = entries(&ds, Tab::Vehicles, None);
        assert_eq!(all.len(), ds.vehicles.len());
        for y in [1975, 1990, 2020] {
            for (k, ..) in entries(&ds, Tab::Vehicles, Some(y)) {
                assert!(ds.vehicles[k].in_service.contains(y));
            }
        }
        let early = entries(&ds, Tab::Vehicles, Some(1970)).len();
        let late = entries(&ds, Tab::Vehicles, Some(2030)).len();
        assert!(early < all.len() || late < all.len(), "the filter removes something");
        assert_eq!(next_year(Some(2030)), None);
    }

    fn plain(armour_mm: f64) -> Front {
        Front { armour_mm, pen_factor: 1.0 }
    }

    #[test]
    fn defeat_range_is_monotonic_in_armour() {
        let ds = ds();
        let a = ds.ammo.iter().find(|a| a.kind == sim_data::AmmoKind::Apfsds && a.max_range_m.is_none()).unwrap();
        let thin = defeat_range(&ds, a, plain(50.0));
        assert_eq!(thin, Some(5000.0), "50 mm is defeated at any range");
        assert_eq!(range_text(thin, a), "ANY RANGE");
        assert_eq!(defeat_range(&ds, a, plain(5000.0)), None);
        let mid = gunnery::penetration_mm(&ds.penetration, a, gunnery::velocity_at(a, 2000.0));
        let r = defeat_range(&ds, a, plain(mid)).unwrap();
        assert!((r - 2000.0).abs() <= 50.0, "{r}");
    }

    #[test]
    fn defeat_range_stops_at_the_rounds_maximum_range() {
        let ds = ds();
        let mut a = ds.ammo.iter().find(|a| a.kind == sim_data::AmmoKind::Apfsds).unwrap().clone();
        a.max_range_m = Some(sim_data::Stat {
            value: 2000.0,
            source: "test".into(),
            confidence: sim_data::Confidence::Estimate,
            notes: None,
        });
        let r = defeat_range(&ds, &a, plain(50.0));
        assert_eq!(r, Some(2000.0), "never reports a range beyond the maximum");
        assert_eq!(range_text(r, &a), "TO 2.0 KM (MAX RANGE)");
        a.max_range_m.as_mut().unwrap().value = 3750.0;
        assert_eq!(range_text(Some(3750.0), &a), "TO 3.75 KM (MAX RANGE)", "no rounding past the stat");
        a.max_range_m.as_mut().unwrap().value = 2000.0;
        // Armour it only defeats closer than the limit keeps the plain wording.
        let mid = gunnery::penetration_mm(&ds.penetration, &a, gunnery::velocity_at(&a, 1000.0));
        assert_eq!(range_text(defeat_range(&ds, &a, plain(mid)), &a), "TO 1.0 KM");
    }

    #[test]
    fn every_listed_round_is_capped_by_its_maximum_range() {
        let ds = ds();
        for a in ds.ammo.iter().filter(|a| a.max_range_m.is_some()) {
            if let Some(r) = defeat_range(&ds, a, plain(1.0)) {
                assert!(r <= a.max_range_m.as_ref().unwrap().get(), "{}: {r}", a.name);
            }
        }
    }

    #[test]
    fn era_weakens_shaped_charges_and_adds_to_kinetic_armour() {
        let ds = ds();
        let v = ds.vehicles.iter().find(|v| v.armour.era.is_some()).expect("an ERA vehicle");
        let era = v.armour.era.as_ref().unwrap();
        let ke = ds.ammo.iter().find(|a| a.kind == sim_data::AmmoKind::Apfsds).unwrap();
        let ce = ds.ammo.iter().find(|a| a.kind.is_chemical()).unwrap();
        let mm = v.armour.turret_front.get();
        let f = front_vs(ke, v, mm);
        assert_eq!((f.armour_mm, f.pen_factor), (mm + era.ke_gain_mm.get(), 1.0));
        let f = front_vs(ce, v, mm);
        assert_eq!(f.armour_mm, mm * v.armour.ce_factor.get());
        assert!((f.pen_factor - (1.0 - era.ce_defeat.get())).abs() < 1e-12);
        // Without ERA, nothing changes.
        let mut bare = v.clone();
        bare.armour.era = None;
        assert_eq!(front_vs(ce, &bare, mm).pen_factor, 1.0);
        assert_eq!(front_vs(ke, &bare, mm).armour_mm, mm);
        // The reach against the ERA front is never longer than against the bare one.
        let with = defeat_range(&ds, ce, front_vs(ce, v, mm));
        let without = defeat_range(&ds, ce, front_vs(ce, &bare, mm));
        assert!(with.unwrap_or(-1.0) <= without.unwrap_or(-1.0), "{with:?} {without:?}");
    }
}
