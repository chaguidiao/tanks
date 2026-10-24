//! M8b-a infantry core (docs/plans/M8B-INFANTRY.md): squads, carriers, dismount and mount,
//! detection, small arms and buildings.

mod common;

use ai::picture::{Condition, PlatoonId, SquadPosture};
use common::*;
use sim::{Command, Event, Replay, SideId, Sim};

const BLUE: SideId = SideId(0);

/// Blue: an M2 platoon of two, each carrying a squad; Red far away.
fn mech_sim() -> Sim {
    let ds = data();
    let m = map(&ds, 400, 800, "grassland", |_, _| 0.0);
    let blue = vec![
        unit("1-1", "ward_m2", [600.0, 400.0], 0.0, Some("1")),
        unit("1-2", "ward_m2", [700.0, 400.0], 0.0, Some("1")),
        squad("1-1A", "rifle_squad_b", [0.0, 0.0], None, Some("1-1")),
        squad("1-2A", "rifle_squad_b", [0.0, 0.0], None, Some("1-2")),
    ];
    let red = vec![unit("211", "kurgan_b", [800.0, 3000.0], 180.0, Some("21"))];
    let mut sim = Sim::new(&scenario(10 * 3600, blue, red), ds, m).unwrap();
    sim.set_ai_enabled(SideId(1), false);
    sim
}

#[test]
fn mounted_squads_form_a_dismount_platoon_and_ride_with_their_carriers() {
    let mut sim = mech_sim();
    let pic = sim.perceived(BLUE);
    let d = pic.platoons.iter().find(|p| p.callsign == "1-D").expect("dismount platoon 1-D");
    assert_eq!(d.members.len(), 2);
    let sq = pic.own.iter().find(|u| u.callsign == "1-1A").unwrap();
    let v = sq.squad.as_ref().expect("squad view");
    assert_eq!((v.strength, v.posture), ((6, 6), SquadPosture::Mounted));
    assert_eq!(v.carrier, Some(sim.truth().id[0]));
    let carrier = sim.truth().id[0];
    sim.submit_immediate(
        BLUE,
        Command::Move { unit: carrier, destination: [600.0, 1400.0], via: vec![], speed_limit_mps: None },
    );
    run(&mut sim, 30.0);
    let w = sim.truth();
    assert!(w.transform[0].pos[1] > 500.0, "the carrier moved");
    assert_eq!(w.transform[2].pos, w.transform[0].pos, "the squad rides with it");
    assert!(w.mounted(2) && !w.mounted(0));
}

#[test]
fn a_carrier_cannot_take_more_men_than_it_has_seats() {
    let ds = data();
    let m = map(&ds, 100, 100, "grassland", |_, _| 0.0);
    let blue = vec![
        unit("1-1", "ward_m2", [200.0, 200.0], 0.0, Some("1")),
        squad("1-1A", "rifle_squad_b", [0.0, 0.0], None, Some("1-1")),
        squad("1-1B", "rifle_squad_b", [0.0, 0.0], None, Some("1-1")),
    ];
    let err = Sim::new(&scenario(36000, blue, vec![]), ds, m).err().expect("over capacity");
    assert!(err.contains("no room"), "{err}");
}

#[test]
fn squads_need_a_carrier_listed_first_and_cannot_ride_in_a_tank() {
    let ds = data();
    let m = map(&ds, 100, 100, "grassland", |_, _| 0.0);
    let blue = vec![
        unit("1-1", "bulwark_a1", [200.0, 200.0], 0.0, Some("1")),
        squad("1-1A", "rifle_squad_b", [0.0, 0.0], None, Some("1-1")),
    ];
    let err = Sim::new(&scenario(36000, blue, vec![]), ds.clone(), m.clone()).err().unwrap();
    assert!(err.contains("carries no troops"), "{err}");
    let blue = vec![squad("1-1A", "rifle_squad_b", [0.0, 0.0], None, Some("1-1"))];
    let err = Sim::new(&scenario(36000, blue, vec![]), ds, m).err().unwrap();
    assert!(err.contains("listed before"), "{err}");
}

#[test]
fn squads_in_a_destroyed_carrier_are_lost_with_it() {
    // A T-72B shoots at the M2 from 900 m until it burns; the squad inside shares its fate
    // (plan §2: catastrophic kill → the data's catastrophic fraction of the men).
    let ds = data();
    let m = map(&ds, 300, 500, "road", |_, _| 0.0);
    let blue = vec![
        unit("1-1", "ward_m2", [600.0, 400.0], 0.0, Some("1")),
        squad("1-1A", "rifle_squad_b", [0.0, 0.0], None, Some("1-1")),
    ];
    let red = vec![unit("211", "kurgan_b", [600.0, 1300.0], 180.0, Some("21"))];
    let mut sim = Sim::new(&scenario(10 * 3600, blue, red), ds, m).unwrap();
    sim.set_ai_enabled(SideId(1), false);
    let mut lost = 0u32;
    for _ in 0..(240 * sim::TICK_HZ) {
        for e in sim.step() {
            if let Event::Casualties { lost: n, .. } = e {
                lost += n as u32;
            }
        }
        if sim.truth().condition[0] == Condition::Destroyed {
            break;
        }
    }
    let w = sim.truth();
    assert_eq!(w.condition[0], Condition::Destroyed, "the T-72B kills the M2 within 4 minutes");
    let s = w.squad[1].as_ref().unwrap();
    assert!(lost > 0 && s.strength < 6, "the squad lost men with its carrier ({lost})");
    assert!(!w.mounted(1), "nobody rides in a burning carrier");
}

#[test]
fn replays_from_another_sim_version_are_rejected_cleanly() {
    let mut sim = mech_sim();
    run(&mut sim, 2.0);
    let rec = Replay::record(&sim);
    assert_eq!(rec.sim_version, sim::SIM_VERSION);
    assert!(Replay::from_ron(&rec.to_ron()).is_ok());
    // A replay saved before versioning has no field and reads as version 0.
    let old = rec.to_ron().replace(&format!("sim_version: {},", sim::SIM_VERSION), "");
    let err = Replay::from_ron(&old).expect_err("old replay rejected");
    assert!(err.contains("sim version 0") && err.contains(&format!("this build is {}", sim::SIM_VERSION)), "{err}");
}

#[test]
fn dismount_platoons_name_their_carrier_platoon() {
    let sim = mech_sim();
    let views = &sim.perceived(BLUE).platoons;
    assert!(views.iter().any(|p| p.callsign == "1" && p.id == PlatoonId(1)));
    assert!(views.iter().any(|p| p.callsign == "1-D"));
}

fn reports(ev: &[Event], want: &sim::PlatoonReport) -> bool {
    ev.iter().any(|e| matches!(e, Event::PlatoonReport { report, .. } if report == want))
}

#[test]
fn an_ifv_platoon_dismounts_and_remounts_within_the_drill_times() {
    // TODO B1/B14 exit: dismount and remount by player command. M2: 20 s ramp drill, 30 s to
    // mount (data), plus the walk between the ramp and the security positions 20 m out.
    let mut sim = mech_sim();
    sim.submit_immediate(BLUE, Command::Dismount { platoon: PlatoonId(1) });
    let ev = run(&mut sim, 15.0);
    assert!(sim.truth().mounted(2) && sim.truth().mounted(3), "still in the ramp drill at 15 s");
    assert!(!reports(&ev, &sim::PlatoonReport::Dismounted));
    let ev = run(&mut sim, 10.0);
    assert!(reports(&ev, &sim::PlatoonReport::Dismounted), "dismounted by 25 s");
    run(&mut sim, 60.0);
    let w = sim.truth();
    for (sq, c) in [(2, 0), (3, 1)] {
        assert!(!w.mounted(sq));
        let d = dist(w.transform[sq].pos, w.transform[c].pos);
        assert!((15.0..=26.0).contains(&d), "squad {sq} in security {d:.1} m from its carrier");
        let s = w.squad[sq].as_ref().unwrap();
        assert_eq!(s.posture, sim::world::Posture::Halted);
    }
    // Remount: walk 20 m back (about 30 s at 2.4 km/h) and the 30 s drill.
    sim.submit_immediate(BLUE, Command::Mount { platoon: PlatoonId(2) });
    let ev = run(&mut sim, 90.0);
    assert!(sim.truth().mounted(2) && sim.truth().mounted(3), "both squads back aboard in 90 s");
    assert!(reports(&ev, &sim::PlatoonReport::Mounted));
    let view = sim.perceived(BLUE).own.iter().find(|u| u.callsign == "1-2A").unwrap().squad.clone().unwrap();
    assert_eq!(view.posture, SquadPosture::Mounted);
}

#[test]
fn the_ramp_drill_waits_for_the_carrier_to_stop() {
    let mut sim = mech_sim();
    let c = sim.truth().id[0];
    sim.submit_immediate(
        BLUE,
        Command::Move { unit: c, destination: [600.0, 2400.0], via: vec![], speed_limit_mps: None },
    );
    run(&mut sim, 20.0);
    let v0 = sim.truth().mobility[0].as_ref().unwrap().speed;
    assert!(v0 > 5.0, "moving at {v0:.1} m/s");
    sim.submit_immediate(BLUE, Command::Dismount { platoon: PlatoonId(1) });
    run(&mut sim, 22.0);
    // Braking from speed takes a few seconds, so the 20 s drill is not over yet.
    assert!(sim.truth().mounted(2), "the drill counts only once stopped");
    run(&mut sim, 10.0);
    assert!(!sim.truth().mounted(2));
    assert!(sim.truth().mobility[0].as_ref().unwrap().speed < 0.5, "the carrier stays halted");
}

#[test]
fn a_dismount_platoon_moves_on_line_at_walking_pace() {
    let mut sim = mech_sim();
    sim.submit_immediate(BLUE, Command::Dismount { platoon: PlatoonId(1) });
    run(&mut sim, 60.0);
    let dest = [650.0, 700.0];
    sim.submit_immediate(
        BLUE,
        Command::PlatoonMove {
            platoon: PlatoonId(2),
            destination: dest,
            via: vec![],
            formation: None,
            technique: sim_data::Technique::Traveling,
            speed_limit_mps: None,
        },
    );
    let ev = run(&mut sim, 60.0);
    let w = sim.truth();
    let v = w.squad[2].as_ref().unwrap().speed;
    // FM 3-21.18: 2.4 km/h cross-country march rate on grassland.
    assert!((v - 0.67).abs() < 0.05, "walking at {v:.2} m/s");
    assert_eq!(w.squad[2].as_ref().unwrap().posture, sim::world::Posture::Moving);
    assert!(!reports(&ev, &sim::PlatoonReport::SetAt { pos: dest }));
    let ev = run(&mut sim, 600.0);
    assert!(reports(&ev, &sim::PlatoonReport::SetAt { pos: dest }), "the platoon reports set");
    let w = sim.truth();
    let gap = dist(w.transform[2].pos, w.transform[3].pos);
    assert!((40.0..=60.0).contains(&gap), "squads 50 m apart on line ({gap:.0} m)");
    assert!(dist(w.transform[2].pos, dest) < 40.0);
}

#[test]
fn a_halted_squad_digs_in_after_the_dig_in_time() {
    let ds = data();
    let m = map(&ds, 100, 100, "grassland", |_, _| 0.0);
    let blue = vec![squad("A", "rifle_squad_b", [200.0, 200.0], Some("1-D"), None)];
    let mut sim = Sim::new(&scenario(36000, blue, vec![]), ds, m).unwrap();
    run(&mut sim, 60.0);
    assert_eq!(sim.truth().squad[0].as_ref().unwrap().posture, sim::world::Posture::Halted);
    run(&mut sim, 1200.0);
    let v = sim.perceived(BLUE).own[0].squad.clone().unwrap();
    assert_eq!(v.posture, SquadPosture::DugIn, "20 minutes halted (data: dig_in_s)");
}
