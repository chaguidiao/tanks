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

/// A Red T-72B watching one Blue squad `range` metres north on `cover`, halted.
fn watch_sim(cover: &str, month: u8, range: f64) -> Sim {
    let ds = data();
    let m = map(&ds, 300, 600, cover, |_, _| 0.0);
    let blue = vec![squad("A", "rifle_squad_b", [600.0, 200.0 + range], Some("1-D"), None)];
    let red = vec![unit("211", "kurgan_b", [600.0, 200.0], 0.0, Some("21"))];
    let mut sc = scenario(10 * 3600, blue, red);
    sc.date.1 = month;
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    sim.set_ai_enabled(SideId(1), false);
    sim
}

#[test]
fn a_tank_crew_reports_a_squad_as_infantry_once_recognised() {
    let mut sim = watch_sim("grassland", 6, 400.0);
    let mut seen = None;
    for _ in 0..(120 * sim::TICK_HZ) {
        sim.step();
        if let Some(c) = sim.perceived(SideId(1)).contacts.first()
            && c.level >= ai::picture::ContactLevel::Identified
        {
            seen = Some(c.clone());
            break;
        }
    }
    let c = seen.expect("identified within 2 minutes at 400 m");
    assert_eq!(c.kind, Some(ai::picture::ContactKind::Infantry));
    assert_eq!(c.type_name.as_deref(), Some("RIFLE SQUAD (MECH)"));
}

#[test]
fn squads_see_vehicles_too() {
    let mut sim = watch_sim("grassland", 6, 600.0);
    run(&mut sim, 60.0);
    assert!(sim.truth().sensors[0].as_ref().unwrap().track(1).is_some(), "the squad tracks the T-72B");
    assert!(sim.perceived(BLUE).contacts.iter().any(|c| c.kind == Some(ai::picture::ContactKind::Vehicle)));
}

#[test]
fn standing_crops_hide_a_halted_squad_in_summer() {
    // TODO B8: crop land in season conceals kneeling and prone men (data: tall_crops).
    let visible = |month: u8| {
        let mut sim = watch_sim("cropland", month, 300.0);
        for _ in 0..(120 * sim::TICK_HZ) {
            sim.step();
            if let Some(tr) = sim.truth().sensors[1].as_ref().unwrap().track(0) {
                return tr.visible_fraction;
            }
        }
        0.0
    };
    let (summer, spring) = (visible(7), visible(3));
    assert!(spring > 0.0 && summer > 0.0, "seen either way at 300 m");
    let want = data().infantry.tall_crops.visible_fraction.get();
    assert!((summer / spring - want).abs() < 1e-9, "July {summer:.3} vs March {spring:.3}");
}

#[test]
fn a_mounted_squad_is_neither_seen_nor_seeing() {
    let ds = data();
    let m = map(&ds, 300, 600, "grassland", |_, _| 0.0);
    let blue = vec![
        unit("1-1", "ward_m2", [600.0, 900.0], 180.0, None),
        squad("1-1A", "rifle_squad_b", [0.0, 0.0], Some("1-D"), Some("1-1")),
    ];
    // No platoons: a platoon's fire distribution would assign targets despite WEAPONS HOLD.
    let red = vec![unit("211", "kurgan_b", [600.0, 200.0], 0.0, None)];
    let mut sim = Sim::new(&scenario(10 * 3600, blue, red), ds, m).unwrap();
    sim.set_ai_enabled(SideId(1), false);
    for (side, k) in [(SideId(1), 2), (BLUE, 0)] {
        let to = sim::Addressee::Unit(sim.truth().id[k]);
        sim.submit_immediate(side, Command::WeaponsControl { to, status: ai::picture::WeaponsStatus::Hold });
    }
    run(&mut sim, 60.0);
    let w = sim.truth();
    assert!(w.sensors[2].as_ref().unwrap().track(0).is_some(), "the T-72B sees the M2");
    assert!(w.sensors[2].as_ref().unwrap().track(1).is_none(), "but not the squad inside");
    assert!(w.sensors[1].as_ref().unwrap().tracks.is_empty(), "the squad inside sees nothing");
}

// --- fire against infantry (plan §4, steps 9–11) ---------------------------------------------

fn squad_strength(sim: &Sim, k: usize) -> u8 {
    sim.truth().squad[k].as_ref().unwrap().strength
}

/// Blue tanks at y = 200 facing a Red squad `range` m north (halted, or in a masonry house).
fn tank_vs_squad(tanks: usize, range: f64, in_house: bool, seed: u64) -> Sim {
    let ds = data();
    let y = 200.0 + range;
    let m = map_with(&ds, 300, 400, "road", |p| {
        p.features.buildings.push(building(1, 594.0, y - 6.0, 606.0, y + 6.0, sim_data::BuildingClass::Masonry));
    });
    let blue = (0..tanks)
        .map(|i| unit(&format!("1-{}", i + 1), "bulwark_a1", [580.0 + i as f64 * 40.0, 200.0], 0.0, None))
        .collect();
    // On the ground beside the house, or (after occupying) inside it.
    let at = if in_house { [600.0, y - 14.0] } else { [560.0, y] };
    let red = vec![squad("A", "mr_squad", at, Some("2-D"), None)];
    let mut sc = scenario(10 * 3600, blue, red);
    sc.seed = seed;
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    sim.set_ai_enabled(SideId(1), false);
    let k = tanks;
    if in_house {
        let sq = sim.truth().id[k];
        sim.submit_immediate(SideId(1), Command::OccupyBuilding { to: sim::Addressee::Unit(sq), building: 1 });
        // Hold fire on the way in and until the tanks watch the house.
        for i in 0..tanks {
            let to = sim::Addressee::Unit(sim.truth().id[i]);
            sim.submit_immediate(BLUE, Command::WeaponsControl { to, status: ai::picture::WeaponsStatus::Hold });
        }
        run(&mut sim, 60.0);
        assert_eq!(sim.truth().squad[k].as_ref().unwrap().in_building, Some(1), "the squad occupied the house");
        for i in 0..tanks {
            let to = sim::Addressee::Unit(sim.truth().id[i]);
            sim.submit_immediate(BLUE, Command::WeaponsControl { to, status: ai::picture::WeaponsStatus::Free });
        }
    }
    sim
}

#[test]
fn a_tank_platoon_defeats_a_squad_in_the_open_with_its_coax() {
    let mut sim = tank_vs_squad(4, 600.0, false, 7);
    let mut volleys = 0;
    for _ in 0..(300 * sim::TICK_HZ) {
        for e in sim.step() {
            if matches!(e, Event::Volley { unit, .. } if unit.0 <= 4) {
                volleys += 1;
            }
        }
    }
    let s = sim.truth().squad[4].as_ref().unwrap();
    assert!(volleys > 0, "the tanks used their coax");
    assert!(s.ineffective, "the squad is combat-ineffective after 5 minutes ({}/{})", s.strength, s.men);
    let main_gun: u32 = sim.truth().gun[0].as_ref().unwrap().stock.iter().sum();
    let full: u32 = sim.truth().gun[0].as_ref().unwrap().capacity.iter().sum();
    assert_eq!(main_gun, full, "no main-gun rounds spent on infantry inside coax range");
}

#[test]
fn tanks_cannot_clear_a_masonry_house_without_he() {
    // Coax only (HE removed): the walls stop most of it.
    let mut sim = tank_vs_squad(2, 500.0, true, 3);
    for i in 0..2 {
        let g = sim.truth_mut().gun[i].as_mut().unwrap();
        g.stock = g.stock.iter().map(|_| 0).collect();
        g.stock[1] = 10; // sabot only
    }
    run(&mut sim, 240.0);
    let s = sim.truth().squad[2].as_ref().unwrap();
    assert!(!s.ineffective, "coax and sabot do not clear the house in 4 minutes ({}/{})", s.strength, s.men);
    // With multipurpose HEAT the same house is cleared.
    let mut sim = tank_vs_squad(2, 500.0, true, 3);
    run(&mut sim, 240.0);
    let s = sim.truth().squad[2].as_ref().unwrap();
    assert!(s.ineffective, "HEAT-MP clears the house ({}/{})", s.strength, s.men);
    let spent = sim.truth().gun[0].as_ref().unwrap();
    assert!(spent.stock.iter().sum::<u32>() < spent.capacity.iter().sum::<u32>(), "main-gun rounds were fired");
}

/// A Blue squad against a Red squad 250 m away (in the open or in a masonry house); Red holds
/// fire when `red_fires` is false. Returns (Blue lost, Red lost, volleys by Blue, by Red).
fn squad_duel(in_house: bool, red_fires: bool, seed: u64) -> (u32, u32, u32, u32) {
    let ds = data();
    let m = map_with(&ds, 300, 300, "road", |p| {
        p.features.buildings.push(building(1, 594.0, 444.0, 606.0, 456.0, sim_data::BuildingClass::Masonry));
    });
    let blue = vec![squad("B", "rifle_squad_b", [600.0, 200.0], Some("1-D"), None)];
    let red = vec![squad("A", "mr_squad", if in_house { [600.0, 436.0] } else { [560.0, 450.0] }, Some("2-D"), None)];
    let mut sc = scenario(10 * 3600, blue, red);
    sc.seed = seed;
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    let (b, r) = (sim::Addressee::Unit(sim.truth().id[0]), sim::Addressee::Unit(sim.truth().id[1]));
    if in_house {
        sim.submit_immediate(SideId(1), Command::OccupyBuilding { to: r, building: 1 });
        sim.submit_immediate(BLUE, Command::WeaponsControl { to: b, status: ai::picture::WeaponsStatus::Hold });
    }
    if !red_fires {
        sim.submit_immediate(SideId(1), Command::WeaponsControl { to: r, status: ai::picture::WeaponsStatus::Hold });
    }
    if in_house {
        run(&mut sim, 40.0);
        assert_eq!(sim.truth().squad[1].as_ref().unwrap().in_building, Some(1));
        sim.submit_immediate(BLUE, Command::WeaponsControl { to: b, status: ai::picture::WeaponsStatus::Free });
    }
    let (mut vb, mut vr) = (0, 0);
    for _ in 0..(300 * sim::TICK_HZ) {
        for e in sim.step() {
            if let Event::Volley { unit, .. } = e {
                if unit.0 == 1 { vb += 1 } else { vr += 1 }
            }
        }
    }
    ((6 - squad_strength(&sim, 0)) as u32, (7 - squad_strength(&sim, 1)) as u32, vb, vr)
}

#[test]
fn squads_trade_fire_and_take_losses() {
    let (mut lost, mut vb, mut vr) = (0, 0, 0);
    for seed in 1..=3 {
        let (b, r, x, y) = squad_duel(false, true, seed);
        lost += b + r;
        vb += x;
        vr += y;
    }
    assert!(vb > 0 && vr > 0, "both squads fire ({vb} / {vr} volleys)");
    assert!(lost > 0, "a 5-minute firefight at 250 m costs men");
}

#[test]
fn a_squad_in_a_house_outlasts_one_in_the_open() {
    // The same Blue squad fires at a Red squad that does not answer: in a masonry house the walls
    // and windows cut Red's losses (FM 90-10-1 ch. 7; data: protection, in_building signature).
    let losses = |in_house: bool| (1..=4).map(|seed| squad_duel(in_house, false, seed).1).sum::<u32>();
    let (open, house) = (losses(false), losses(true));
    assert!(open >= 4, "squads in the open take losses ({open})");
    assert!(house * 3 <= open, "the house protects: {house} lost inside vs {open} in the open");
}

#[test]
fn machine_gun_fire_pins_a_squad() {
    let mut sim = tank_vs_squad(1, 500.0, false, 11);
    assert!(sim.truth().squad[1].is_some());
    let mut peak: f64 = 0.0;
    for _ in 0..(90 * sim::TICK_HZ) {
        sim.step();
        peak = peak.max(sim.truth().suppression[1]);
    }
    let pinned = data().infantry.pinned_above.get();
    assert!(peak > pinned, "coax bursts pin the squad (peak suppression {peak:.2})");
}

#[test]
fn a_building_holds_only_as_many_squads_as_its_floor_area_allows() {
    let ds = data();
    // 9 × 7 m, one floor: 63 m² → one squad (TerrainData::squad_capacity).
    let m = map_with(&ds, 200, 200, "grassland", |p| {
        p.features.buildings.push(building(1, 396.0, 396.0, 405.0, 403.0, sim_data::BuildingClass::Masonry));
    });
    let blue = vec![
        squad("A", "rifle_squad_b", [300.0, 300.0], Some("1-D"), None),
        squad("B", "rifle_squad_b", [340.0, 300.0], Some("1-D"), None),
    ];
    let mut sim = Sim::new(&scenario(36000, blue, vec![]), ds, m).unwrap();
    sim.submit_immediate(BLUE, Command::OccupyBuilding { to: sim::Addressee::Platoon(PlatoonId(1)), building: 1 });
    run(&mut sim, 300.0);
    let inside = (0..2).filter(|&k| sim.truth().squad[k].as_ref().unwrap().in_building == Some(1)).count();
    assert_eq!(inside, 1);
    let view = sim.perceived(BLUE).own.iter().find(|u| u.squad.as_ref().is_some_and(|s| s.building.is_some())).cloned();
    assert_eq!(view.unwrap().squad.unwrap().posture, SquadPosture::InBuilding);
}

#[test]
fn a_squad_under_fire_in_the_open_runs_for_cover() {
    // ATP 3-21.8 battle drill 2: a squad walking along open ground is fired on by a coax and goes
    // to the wood 30 m north of its route.
    let ds = data();
    let forest = ds.terrain.landcover_id("deciduous_forest").unwrap();
    let m = map_with(&ds, 300, 300, "grassland", |p| {
        for j in 0..300 {
            for i in 0..300 {
                if (j as f64 + 0.5) * 4.0 >= 730.0 {
                    p.landcover.set(i, j, forest);
                }
            }
        }
    });
    let blue = vec![squad("A", "rifle_squad_b", [300.0, 700.0], Some("1-D"), None)];
    let red = vec![unit("211", "kurgan_b", [500.0, 150.0], 0.0, None)];
    let mut sim = Sim::new(&scenario(10 * 3600, blue, red), ds, m).unwrap();
    sim.set_ai_enabled(SideId(1), false);
    let sq = sim.truth().id[0];
    sim.submit_immediate(
        BLUE,
        Command::Move { unit: sq, destination: [900.0, 700.0], via: vec![], speed_limit_mps: None },
    );
    let mut reacted = false;
    for _ in 0..(120 * sim::TICK_HZ) {
        sim.step();
        if sim.truth().transform[0].pos[1] >= 728.0 {
            reacted = true;
            break;
        }
    }
    // It may be pinned on the way (suppressed squads do not move), but it has left its route.
    let at = sim.truth().transform[0].pos;
    assert!(reacted || at[1] > 705.0, "the squad turned for the wood; at {at:?}");
    assert!(sim.truth().ai[0].as_ref().unwrap().held.is_some(), "its move is held for later");
}
