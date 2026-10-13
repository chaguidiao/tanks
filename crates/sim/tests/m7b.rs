//! M7b acceptance tests: round types (KE/HEAT/HE/APDS/ATGM), CE vs KE armour, explosive
//! reactive armour, ammunition selection by target class, and tank-gun smoke missions.

mod common;

use common::*;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;
use sim::gunnery::{self, GunState, ShotGeometry};
use sim::{Addressee, Command, SideId, Sim};
use sim_data::{AmmoKind, DataSet};

const BLUE: SideId = SideId(0);
const DAY: u32 = 10 * 3600;

fn loadout(ds: &DataSet, v: &str) -> Vec<usize> {
    let def = ds.vehicle(v).unwrap();
    def.armament.load.iter().map(|l| ds.ammo.iter().position(|a| a.id == l.ammo).unwrap()).collect()
}

/// A shot at the target's front from `range`, target facing the shooter.
fn frontal(range: f64) -> ShotGeometry {
    ShotGeometry {
        range,
        visible: 1.0,
        bearing_to_shooter: 0.0,
        target_heading: 0.0,
        crossing_speed: 0.0,
        shooter_moving: false,
    }
}

/// Fraction of 3000 shots that penetrate, and fraction on which a tile fired.
fn penetration_rate(ds: &DataSet, shooter: &str, round: &str, target: &str, era: u32) -> (f64, f64) {
    let (s, t, a) = (ds.vehicle(shooter).unwrap(), ds.vehicle(target).unwrap(), ds.ammo(round).unwrap());
    let g = frontal(1000.0);
    let mut rng = ChaCha8Rng::seed_from_u64(11);
    let (mut hits, mut pens, mut tiles) = (0, 0, 0);
    for _ in 0..3000 {
        let o = gunnery::resolve(s, t, era, a, &ds.penetration, &ds.damage, &g, (0.0, 0.0), &mut rng);
        if o.hit {
            hits += 1;
            pens += o.penetrated as u32;
            tiles += o.era_fired as u32;
        }
    }
    assert!(hits > 500, "enough hits to measure: {hits}");
    (pens as f64 / hits as f64, tiles as f64 / hits as f64)
}

#[test]
fn era_defeats_shaped_charges_far_more_than_it_helps_against_sabot() {
    let ds = data();
    // The red kurgan carries ERA in 1990. A missile (650 mm) beats its CE-equivalent hull front
    // (480 x 1.3) bare, but not once a tile fires; HEAT-MP (480 mm) never beats the front at all.
    let (heat_front, _) = penetration_rate(&ds, "bulwark_a1", "heat_120a", "kurgan_b", 0);
    assert_eq!(heat_front, 0.0, "a tank gun's HEAT cannot defeat the kurgan's frontal CE protection");
    let (heat_bare, _) = penetration_rate(&ds, "skif_m1", "atgm_sk5", "kurgan_b", 0);
    let (heat_era, tiles) = penetration_rate(&ds, "skif_m1", "atgm_sk5", "kurgan_b", 90);
    assert!(heat_bare > 0.1, "the missile can defeat the hull front bare: {heat_bare}");
    assert!(tiles > 0.3, "most frontal hits strike a tile: {tiles}");
    assert!(heat_era < heat_bare * 0.6, "ERA cuts missile penetration: {heat_bare} -> {heat_era}");
    let (sabot_bare, _) = penetration_rate(&ds, "bulwark_a1", "apfsds_120a", "kurgan_b", 0);
    let (sabot_era, _) = penetration_rate(&ds, "bulwark_a1", "apfsds_120a", "kurgan_b", 90);
    assert!(sabot_era <= sabot_bare + 0.01);
    assert!(heat_bare - heat_era > sabot_bare - sabot_era, "ERA hurts shaped charges more than sabot");
}

#[test]
fn spent_tiles_stop_working() {
    let ds = data();
    let (_, spent) = penetration_rate(&ds, "skif_m1", "atgm_sk5", "kurgan_b", 0);
    assert_eq!(spent, 0.0, "no tiles left, none fire");
}

#[test]
fn he_cannot_hurt_a_tank_but_defeats_a_recon_vehicle() {
    let ds = data();
    let (vs_tank, _) = penetration_rate(&ds, "skif_m1", "he_30", "bulwark_a1", 0);
    let (vs_recon, _) = penetration_rate(&ds, "skif_m1", "he_30", "falcon_r1", 0);
    assert_eq!(vs_tank, 0.0);
    assert!(vs_recon > 0.9, "{vs_recon}");
}

#[test]
fn crews_pick_the_round_by_target_class() {
    let ds = data();
    let bulwark = ds.vehicle("bulwark_a1").unwrap();
    let lo = loadout(&ds, "bulwark_a1");
    let g = GunState::new(0.0, bulwark.armament.load.iter().map(|l| l.count).collect());
    let geo = frontal(1500.0);
    let pick = |target: &str, g: &GunState| {
        let i = gunnery::select_round(g, &lo, &ds.ammo, &ds.penetration, ds.vehicle(target).unwrap(), 0, &geo).unwrap();
        ds.ammo[lo[i]].kind
    };
    assert_eq!(pick("kurgan_b", &g), AmmoKind::Apfsds, "sabot against a tank");
    assert_eq!(pick("skif_m1", &g), AmmoKind::Heat, "multipurpose round against an IFV");
    assert_eq!(pick("falcon_r1", &g), AmmoKind::Heat);
    let mut held = g.clone();
    held.selected = Some(AmmoKind::Heat);
    assert_eq!(pick("kurgan_b", &held), AmmoKind::Heat, "the commander's choice wins when it is available");
    let mut dry = g.clone();
    dry.stock[0] = 0;
    assert_eq!(pick("kurgan_b", &dry), AmmoKind::Heat, "an empty rack forces the other type");
    held.stock[1] = 0;
    held.selected = Some(AmmoKind::Heat);
    assert_eq!(pick("kurgan_b", &held), AmmoKind::Apfsds, "a held type with no rounds falls back to auto");
}

#[test]
fn smoke_is_never_selected_against_a_target() {
    let ds = data();
    let lo = loadout(&ds, "bulwark_a1");
    let mut g = GunState::new(0.0, vec![0, 0, 4]);
    g.selected = Some(AmmoKind::Smoke);
    let geo = frontal(1000.0);
    assert!(
        gunnery::select_round(&g, &lo, &ds.ammo, &ds.penetration, ds.vehicle("kurgan_b").unwrap(), 0, &geo).is_none()
    );
}

fn open_sim(blue: Vec<sim_data::UnitDef>, red: Vec<sim_data::UnitDef>) -> Sim {
    let ds = data();
    let m = map(&ds, 600, 600, "cropland", |_, _| 0.0);
    Sim::new(&scenario(DAY, blue, red), ds, m).unwrap()
}

#[test]
fn a_smoke_mission_lays_a_screen_at_the_aim_point_and_spends_rounds() {
    let mut sim = open_sim(vec![unit("B1", "bulwark_a1", [1200.0, 400.0], 0.0, None)], vec![]);
    let id = sim.truth().id[0];
    let smoke_i = sim.vehicle_defs().iter().position(|v| v.id == "bulwark_a1").map(|_| 2).unwrap();
    let before = sim.truth().gun[0].as_ref().unwrap().stock[smoke_i];
    assert_eq!(before, 4);
    sim.submit_immediate(BLUE, Command::FireSmoke { to: Addressee::Unit(id), at: [1200.0, 1000.0], rounds: 2 });
    run(&mut sim, 40.0);
    let g = sim.truth().gun[0].as_ref().unwrap();
    assert_eq!(g.stock[smoke_i], 2, "two smoke rounds fired");
    assert!(g.mission.is_none());
    let clouds = &sim.perceived(BLUE).smoke;
    assert!(!clouds.is_empty());
    assert!(clouds.iter().any(|c| dist(c.pos, [1200.0, 1000.0]) < 120.0), "a cloud sits at the aim point");
}

#[test]
fn a_held_round_type_is_the_one_fired() {
    let mut sim = open_sim(
        vec![unit("B1", "bulwark_a1", [1200.0, 400.0], 0.0, None)],
        vec![unit("R1", "kurgan_b", [1200.0, 1600.0], 180.0, None)],
    );
    let id = sim.truth().id[0];
    sim.submit_immediate(BLUE, Command::SelectAmmo { to: Addressee::Unit(id), kind: Some(AmmoKind::Heat) });
    run(&mut sim, 60.0);
    let g = sim.truth().gun[0].as_ref().unwrap();
    assert_eq!(g.stock[0], 24, "no sabot fired");
    assert!(g.stock[1] < 12, "HEAT fired: {:?}", g.stock);
}

#[test]
fn era_tiles_are_spent_by_hits() {
    let mut sim = open_sim(
        vec![unit("B1", "bulwark_a1", [1200.0, 400.0], 0.0, None)],
        vec![unit("R1", "kurgan_b", [1200.0, 1500.0], 180.0, None)],
    );
    let start = sim.truth().era[1];
    assert_eq!(start, 90, "the 1990 kurgan carries its tiles");
    run(&mut sim, 90.0);
    assert!(sim.truth().era[1] < start, "tiles spent by hits on the front");
}

#[test]
fn the_older_scenario_date_drops_era_from_the_kurgan() {
    let ds = data();
    let m = map(&ds, 100, 100, "cropland", |_, _| 0.0);
    let mut sc = scenario(DAY, vec![], vec![unit("R1", "kurgan_b", [200.0, 200.0], 0.0, None)]);
    sc.date = (1985, 6, 1);
    let sim = Sim::new(&sc, ds, m).unwrap();
    assert_eq!(sim.truth().era[0], 0);
}
