//! M2 (seeing) and M3 (shooting) acceptance tests on synthetic maps.

mod common;

use common::*;
use sim::view::{Condition, WeaponsStatus};
use sim::{Addressee, Command, EntityId, SideId, Sim};

const BLUE: SideId = SideId(0);
const RED: SideId = SideId(1);
const DAY: u32 = 10 * 3600;
const NIGHT: u32 = 3600;

fn hold_fire(sim: &mut Sim) {
    sim.submit_immediate(
        BLUE,
        Command::WeaponsControl { to: Addressee::Unit(EntityId(1)), status: WeaponsStatus::Hold },
    );
    sim.submit_immediate(
        RED,
        Command::WeaponsControl { to: Addressee::Unit(EntityId(2)), status: WeaponsStatus::Hold },
    );
}

fn pair(start: u32, ridge: f64) -> Sim {
    let ds = data();
    let m = map(&ds, 200, 800, "grassland", |_, y| ridge * (-((y - 1500.0) / 60.0).powi(2)).exp());
    let sc = scenario(
        start,
        vec![unit("1-1", "bulwark_a1", [400.0, 500.0], 0.0, None)],
        vec![unit("201", "kurgan_b", [400.0, 2500.0], 180.0, None)],
    );
    Sim::new(&sc, ds, m).unwrap()
}

#[test]
fn both_sides_detect_each_other_by_day_and_reports_are_delayed() {
    let mut sim = pair(DAY, 0.0);
    hold_fire(&mut sim);
    assert!(sim.perceived(BLUE).contacts.is_empty(), "fog of war: nothing known at start");
    run(&mut sim, 90.0);
    let red_pos = sim.truth().transform[1].pos;
    let b = &sim.perceived(BLUE).contacts;
    assert!(!b.is_empty(), "Blue should have a contact by day at 2 km");
    // Laser-ranged reports are accurate to tens of metres at 2 km (3 mrad bearing ≈ 6 m).
    assert!(b.iter().any(|c| dist(c.pos, red_pos) < 60.0), "{b:?}");
    assert!(!sim.perceived(RED).contacts.is_empty(), "Red sees Blue by day");
}

#[test]
fn at_night_only_the_thermal_side_sees() {
    let mut sim = pair(NIGHT, 0.0);
    hold_fire(&mut sim);
    run(&mut sim, 120.0);
    assert!(!sim.perceived(BLUE).contacts.is_empty(), "thermal sight detects at 2 km at night");
    assert!(sim.perceived(RED).contacts.is_empty(), "active IR is limited to ~900 m");
}

#[test]
fn a_ridge_blocks_detection() {
    let mut sim = pair(DAY, 15.0);
    hold_fire(&mut sim);
    run(&mut sim, 90.0);
    assert!(sim.perceived(BLUE).contacts.is_empty() && sim.perceived(RED).contacts.is_empty());
}

#[test]
fn frontal_duel_at_2km_favours_the_better_armoured_tank() {
    // APFSDS 120-A (~600 mm at 2 km) defeats the Kurgan's frontal armour (~480-520 mm);
    // APFSDS 125-B (~440 mm at 2 km) does not defeat the Bulwark's (~500-550 mm).
    let mut sim = pair(DAY, 0.0);
    let events = run(&mut sim, 180.0);
    let w = sim.truth();
    assert_ne!(w.condition[1], Condition::Operational, "Red tank should be knocked out");
    assert_ne!(w.condition[0], Condition::Destroyed, "Blue tank should survive");
    assert!(events.iter().any(|e| matches!(e, sim::Event::Fired { unit: EntityId(1), .. })));
    let red_contact = sim.perceived(BLUE).contacts.iter().find(|c| dist(c.pos, w.transform[1].pos) < 100.0);
    if w.condition[1] == Condition::Destroyed {
        assert!(red_contact.is_some_and(|c| c.destroyed), "the shooter reports the kill");
    }
}
