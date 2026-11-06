//! Fixes from the newcomer playtest of version 0.1.3 (docs/PLAYTEST-2026-11-06.md).

mod common;

use ai::picture::{PlatoonId, PositionKind};
use common::*;
use sim::view::WeaponsStatus;
use sim::{Addressee, Command, Event, PlatoonReport, SideId, Sim};
use sim_data::{Formation, Technique};

const BLUE: SideId = SideId(0);
const RED: SideId = SideId(1);
const P1: PlatoonId = PlatoonId(1);

fn platoon_near_the_edge() -> Sim {
    let ds = data();
    let m = map(&ds, 400, 1000, "cropland", |_, _| 0.0);
    let blue: Vec<_> = (0..4)
        .map(|i| unit(&format!("1-{}", i + 1), "bulwark_a1", [300.0 + i as f64 * 60.0, 2000.0], 270.0, Some("1")))
        .collect();
    let red = vec![unit("211", "kurgan_b", [800.0, 3900.0], 180.0, Some("21"))];
    let mut sim = Sim::new(&scenario(10 * 3600, blue, red), ds, m).unwrap();
    sim.set_ai_enabled(RED, false);
    sim
}

fn go(sim: &mut Sim, to: [f64; 2], formation: Formation) {
    sim.submit_immediate(
        BLUE,
        Command::PlatoonMove {
            platoon: P1,
            destination: to,
            via: vec![],
            formation: Some(formation),
            technique: Technique::Traveling,
            speed_limit_mps: None,
        },
    );
}

/// Off the map or hard against its edge, the platoon must get there and finish the order.
fn arrives(to: [f64; 2], formation: Formation) {
    let mut sim = platoon_near_the_edge();
    go(&mut sim, to, formation);
    let mut set = false;
    for _ in 0..(300.0 / sim::DT) as u32 {
        set |= sim.step().iter().any(
            |e| matches!(e, Event::PlatoonReport { report: PlatoonReport::SetAt { .. }, side, .. } if *side == BLUE),
        );
    }
    assert!(set, "platoon never reported set: {}", sim.perceived(BLUE).platoon(P1).unwrap().task);
    let w = sim.truth();
    for k in 0..4 {
        let p = w.transform[k].pos;
        assert!((0.0..=1600.0).contains(&p[0]) && (0.0..=4000.0).contains(&p[1]), "off the map at {p:?}");
    }
}

#[test]
fn a_move_order_past_the_edge_ends_at_the_edge() {
    arrives([-500.0, 2200.0], Formation::Column);
}

#[test]
fn a_wide_formation_hard_against_the_edge_still_finishes() {
    arrives([6.0, 2200.0], Formation::Line);
}

/// Two Red tanks hold fire: the nearer turned 35 degrees, where the round is just short of its
/// line-of-sight armour (480 mm / cos 35 = 586 mm against about 580 mm at 2.6 km), the farther
/// square on, where the same round gets through.
fn shift_fire_sim(hits_to_shift: Option<f64>) -> (Sim, usize, usize) {
    let mut ds = (*data()).clone();
    if let Some(h) = hits_to_shift {
        for d in &mut ds.doctrine {
            d.no_effect_hits.value = h;
        }
    }
    let ds = std::sync::Arc::new(ds);
    let m = map(&ds, 400, 1000, "cropland", |_, _| 0.0);
    let blue = vec![unit("1-1", "bulwark_a1", [800.0, 200.0], 0.0, Some("1"))];
    let red = vec![
        unit("211", "kurgan_b", [600.0, 2830.0], 215.0, Some("21")),
        unit("212", "kurgan_b", [1000.0, 2900.0], 180.0, Some("21")),
    ];
    let mut sim = Sim::new(&scenario(10 * 3600, blue, red), ds, m).unwrap();
    sim.set_ai_enabled(RED, false);
    sim.submit_immediate(
        RED,
        Command::WeaponsControl { to: Addressee::Platoon(PlatoonId(21)), status: WeaponsStatus::Hold },
    );
    (sim, 1, 2) // world order: Blue's tank, then the two Red ones
}

/// ATP 3-20.15 / TC 3-20.31: a crew that sees its rounds have no effect shifts to a target it can hurt.
#[test]
fn a_crew_shifts_off_a_target_it_cannot_penetrate() {
    let run = |rule: Option<f64>| {
        let (mut sim, near, far) = shift_fire_sim(rule);
        let (near_id, far_id) = (sim.truth().id[near], sim.truth().id[far]);
        let (mut near_hits, mut far_pen) = (0, 0);
        for _ in 0..(200.0 / sim::DT) as u32 {
            for e in sim.step() {
                if let Event::Hit { unit, penetrated, .. } = e {
                    if unit == near_id && !penetrated {
                        near_hits += 1;
                    }
                    if unit == far_id && penetrated {
                        far_pen += 1;
                    }
                }
            }
        }
        (near_hits, far_pen)
    };
    // The control: a crew that never gives up spends its rounds on the near tank.
    let (stuck, control_pen) = run(Some(1000.0));
    assert!(stuck >= 8, "control: only {stuck} hits on the near tank");
    let (_, far_pen) = run(None);
    assert!(far_pen > control_pen, "penetrations on the far tank: {far_pen}, control {control_pen}");
}

/// PT30: the shift is a default the player can turn off, by order or for a leg of a plan.
#[test]
fn a_platoon_ordered_to_stay_on_keeps_firing_at_the_target() {
    let (mut sim, near, far) = shift_fire_sim(None);
    let side = BLUE;
    let platoon = sim.perceived(side).platoons[0].id;
    sim.submit_immediate(side, Command::SetShiftFire { platoon, shift_fire: ai::picture::ShiftFire::Stay });
    let (near_id, far_id) = (sim.truth().id[near], sim.truth().id[far]);
    let (mut near_hits, mut far_hits) = (0, 0);
    for _ in 0..(200.0 / sim::DT) as u32 {
        for e in sim.step() {
            if let Event::Hit { unit, .. } = e {
                near_hits += u32::from(unit == near_id);
                far_hits += u32::from(unit == far_id);
            }
        }
    }
    assert_eq!(sim.perceived(side).platoons[0].shift_fire, ai::picture::ShiftFire::Stay);
    assert!(near_hits >= 8, "stays on the near tank: {near_hits} hits");
    assert_eq!(far_hits, 0, "and leaves the far one alone");
}

/// The fire plan gives the platoon's only tank the near target; an ENGAGE order on the far one
/// must last against it (it used to be overwritten within a second).
#[test]
fn an_engage_order_outlasts_the_fire_plan() {
    // (A crew free to shift would have killed the far tank by now.)
    let (mut sim, _, far) = shift_fire_sim(Some(1000.0));
    let far_pos = sim.truth().transform[far].pos;
    for _ in 0..(60.0 / sim::DT) as u32 {
        sim.step();
    }
    let contact = sim
        .perceived(BLUE)
        .contacts
        .iter()
        .find(|c| ((c.pos[0] - far_pos[0]).powi(2) + (c.pos[1] - far_pos[1]).powi(2)).sqrt() < 100.0);
    let contact = contact.expect("far tank seen").id;
    let unit = sim.truth().id[0];
    sim.submit_immediate(BLUE, Command::Engage { unit, contact });
    for _ in 0..(5.0 / sim::DT) as u32 {
        sim.step();
    }
    assert_eq!(sim.truth().gun[0].as_ref().unwrap().assigned, Some(far));
}

#[test]
fn a_crew_with_nothing_else_to_shoot_at_keeps_firing() {
    let (mut sim, near, far) = shift_fire_sim(None);
    sim.truth_mut().condition[far] = sim::view::Condition::Destroyed;
    let near_id = sim.truth().id[near];
    let mut hits = 0;
    for _ in 0..(120.0 / sim::DT) as u32 {
        hits += sim.step().iter().filter(|e| matches!(e, Event::Hit { unit, .. } if *unit == near_id)).count();
    }
    assert!(hits > 6, "stopped firing after {hits} hits");
}

#[test]
fn the_status_card_lists_only_the_rounds_carried_by_type() {
    let sim = platoon_near_the_edge();
    let pic = sim.perceived(BLUE);
    let u = pic.own.iter().find(|u| u.callsign == "1-1").unwrap();
    let labels: Vec<&str> = u.rounds.iter().map(|r| r.0.as_str()).collect();
    assert_eq!(labels.iter().filter(|l| **l == "APFSDS").count(), 1, "{labels:?}");
    assert_eq!(u.rounds.iter().find(|r| r.0 == "APFSDS").unwrap().1, 24, "only the 1988 round is carried in 1990");
}

#[test]
fn breaking_contact_with_nothing_seen_uses_no_smoke_and_names_no_threat() {
    let mut sim = platoon_near_the_edge();
    sim.submit_immediate(BLUE, Command::BreakContact { platoon: P1 });
    let mut threat = None;
    let mut smoke = 0;
    for _ in 0..20 {
        for e in sim.step() {
            match e {
                Event::PlatoonReport { report: PlatoonReport::BreakingContact { threat: t }, .. } => threat = Some(t),
                Event::SmokeLaunched { .. } => smoke += 1,
                _ => {}
            }
        }
    }
    assert_eq!(threat, Some(None), "reported as breaking contact, with no threat");
    assert_eq!(smoke, 0);
}

#[test]
fn positions_with_no_view_of_the_threat_are_reported_as_masked_not_exposed() {
    // A ridge between the position and the threat: nothing there sees over it.
    let ds = data();
    let m = map(&ds, 400, 1000, "cropland", |_, y| if (1500.0..1700.0).contains(&y) { 40.0 } else { 0.0 });
    let blue: Vec<_> = (0..4)
        .map(|i| unit(&format!("1-{}", i + 1), "bulwark_a1", [600.0 + i as f64 * 60.0, 900.0], 0.0, Some("1")))
        .collect();
    let red = vec![unit("211", "kurgan_b", [800.0, 3900.0], 180.0, Some("21"))];
    let mut sim = Sim::new(&scenario(10 * 3600, blue, red), ds, m).unwrap();
    sim.set_ai_enabled(RED, false);
    sim.submit_immediate(
        BLUE,
        Command::OccupyPosition {
            platoon: P1,
            centre: [800.0, 1000.0],
            radius_m: 150.0,
            facing: 0.0,
            kind: PositionKind::HullDown,
        },
    );
    let mut report = None;
    for _ in 0..(200.0 / sim::DT) as u32 {
        for e in sim.step() {
            if let Event::PlatoonReport { report: PlatoonReport::SetInPosition { exposed, masked, .. }, .. } = e {
                report = Some((exposed, masked));
            }
        }
    }
    let (exposed, masked) = report.expect("set in position");
    let pic = sim.perceived(BLUE);
    let notes = pic.own.iter().filter(|u| u.position_note.as_deref() == Some("MASKED")).count() as u8;
    let exposed_notes = pic.own.iter().filter(|u| u.position_note.as_deref() == Some("EXPOSED")).count() as u8;
    assert_eq!((exposed, masked), (exposed_notes, notes), "the report matches the roster");
}
