//! M8b-b anti-tank teams (docs/plans/M8B-AT-TEAMS.md): squad rockets and missiles, the ambush
//! rules, operator in the loop.

mod common;

use ai::picture::{Condition, WeaponsStatus};
use common::*;
use sim::{Addressee, Command, EntityId, Event, SideId, Sim};

const BLUE: SideId = SideId(0);
const RED: SideId = SideId(1);

fn hold(sim: &mut Sim, side: SideId, unit: EntityId) {
    sim.submit_immediate(side, Command::WeaponsControl { to: Addressee::Unit(unit), status: WeaponsStatus::Hold });
}

/// Shots fired by `unit` (rockets and missiles from squads, guns from vehicles).
fn shots_by(ev: &[Event], unit: EntityId) -> usize {
    ev.iter().filter(|e| matches!(e, Event::Fired { unit: u, .. } if *u == unit)).count()
}

/// A Blue M1 driving from `from` to `to` past a halted Red MR squad (1980: PG-7V and PG-7VL) at
/// (600, 600). The tank holds fire, so the squad is never fired on.
fn tank_past_squad(from: [f64; 2], to: [f64; 2], heading: f64, seed: u64) -> Sim {
    let ds = data();
    let m = map(&ds, 300, 400, "grassland", |_, _| 0.0);
    let blue = vec![unit("1-1", "bulwark_a0", from, heading, None)];
    let red = vec![squad("A", "mr_squad", [600.0, 600.0], Some("2-D"), None)];
    let mut sc = scenario(10 * 3600, blue, red);
    sc.date = (1980, 6, 1);
    sc.seed = seed;
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    sim.set_ai_enabled(RED, false);
    let tank = sim.truth().id[0];
    hold(&mut sim, BLUE, tank);
    sim.submit_immediate(BLUE, Command::Move { unit: tank, destination: to, via: vec![], speed_limit_mps: Some(5.0) });
    sim
}

#[test]
fn an_rpg_holds_fire_on_a_tank_showing_its_front() {
    // Ambush rule (plan §2.2, prefer_flank): the PG-7V and PG-7VL cannot defeat the M1's front,
    // so a squad not yet fired on lets it come straight on, even into the kill zone.
    let mut sim = tank_past_squad([600.0, 1200.0], [600.0, 720.0], 180.0, 1);
    let ev = run(&mut sim, 150.0);
    let w = sim.truth();
    assert!(dist(w.transform[0].pos, [600.0, 720.0]) < 20.0, "the tank closed to 120 m");
    assert!(w.sensors[1].as_ref().unwrap().track(0).is_some(), "the squad saw it");
    assert_eq!(shots_by(&ev, w.id[1]), 0, "no rocket at the front");
}

#[test]
fn an_rpg_fires_at_a_tank_crossing_its_front_inside_the_trigger_range() {
    // The same squad engages a tank passing 150 m away showing its flank (plan §2.2), and some
    // rockets hit (Isby 1988: about two in three at 150 m on a moving target).
    let (mut shots, mut hits) = (0, 0);
    for seed in 1..=6 {
        let mut sim = tank_past_squad([250.0, 750.0], [1000.0, 750.0], 90.0, seed);
        let ev = run(&mut sim, 150.0);
        let squad = sim.truth().id[1];
        shots += shots_by(&ev, squad);
        hits += ev.iter().filter(|e| matches!(e, Event::ShotResult { unit, hit: true, .. } if *unit == squad)).count();
    }
    assert!(shots >= 6, "every run fires at the flank ({shots})");
    assert!(hits > 0, "and hits ({hits} of {shots})");
}

/// A Red Fagot team (1985) on a flat field 1,200 m from a Blue M1 that holds fire; the team is
/// set up already (halted long enough). Returns the sim and the team's world index.
fn missile_post(seed: u64) -> Sim {
    missile_post_at(seed, 1200.0)
}

fn missile_post_at(seed: u64, range: f64) -> Sim {
    let ds = data();
    let m = map(&ds, 300, 500, "grassland", |_, _| 0.0);
    let blue = vec![unit("1-1", "bulwark_a0", [600.0, 400.0], 90.0, None)];
    let red = vec![squad("AT", "at_team_r", [600.0, 400.0 + range], Some("2-D"), None)];
    let mut sc = scenario(10 * 3600, blue, red);
    sc.date = (1985, 6, 1);
    sc.seed = seed;
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    sim.set_ai_enabled(RED, false);
    let tank = sim.truth().id[0];
    hold(&mut sim, BLUE, tank);
    sim.truth_mut().squad[1].as_mut().unwrap().halted_s = 120.0;
    sim
}

/// Run until the team's first missile lands; `pin` pins the gunner as soon as it is in flight.
/// Returns whether that missile hit.
fn first_missile(seed: u64, pin: bool) -> Option<bool> {
    let mut sim = missile_post(seed);
    let team = sim.truth().id[1];
    let mut launched = false;
    for _ in 0..(180 * sim::TICK_HZ) {
        if launched && pin {
            sim.truth_mut().suppression[1] = 1.0;
        }
        for e in sim.step() {
            match e {
                Event::Fired { unit, .. } if unit == team => launched = true,
                Event::ShotResult { unit, hit, .. } if unit == team => return Some(hit),
                _ => {}
            }
        }
    }
    None
}

#[test]
fn a_pinned_gunner_loses_his_missile() {
    // Target A7 (plan §11): a SACLOS missile with its gunner pinned during the flight misses;
    // the same shots hit most of the time when he can keep tracking (Fagot class at 1,200 m).
    let free: Vec<bool> = (1..=8).map(|s| first_missile(s, false).expect("fired")).collect();
    let pinned: Vec<bool> = (1..=8).map(|s| first_missile(s, true).expect("fired")).collect();
    assert!(free.iter().filter(|h| **h).count() >= 6, "tracked to impact: {free:?}");
    assert!(pinned.iter().all(|h| !h), "pinned in flight: {pinned:?}");
}

#[test]
fn a_launch_gives_the_team_away() {
    const RANGE: f64 = 1200.0;
    const SECONDS: u32 = 90;
    // Backblast (plan §2.2, §4): at 1,200 m a dug-in missile team that holds fire stays hidden
    // from the tank crew; once it fires, its launch gives it away.
    let found = |fire: bool| {
        let mut n = 0;
        for seed in 1..=6 {
            let mut sim = missile_post_at(seed, RANGE);
            if let Some(s) = sim.truth_mut().squad[1].as_mut() {
                s.halted_s = 1.0e6; // dug in
            }
            if !fire {
                let team = sim.truth().id[1];
                hold(&mut sim, RED, team);
            }
            for _ in 0..(SECONDS * sim::TICK_HZ) {
                sim.step();
                if sim.truth().sensors[0].as_ref().unwrap().track(1).is_some() {
                    n += 1;
                    break;
                }
            }
        }
        n
    };
    let (fired, quiet) = (found(true), found(false));
    assert!(fired >= 5 && quiet <= 1, "found after firing {fired}/6, holding fire {quiet}/6");
}

#[test]
fn a_destroyed_tank_is_no_target_and_rounds_are_counted() {
    // Stock accounting: every launch spends one round of the load.
    let mut sim = missile_post(3);
    let full = sim.truth().squad[1].as_ref().unwrap().anti_armour[0].rounds();
    let ev = run(&mut sim, 120.0);
    let team = sim.truth().id[1];
    let left = sim.truth().squad[1].as_ref().unwrap().anti_armour[0].rounds();
    assert_eq!((full - left) as usize, shots_by(&ev, team));
    if sim.truth().condition[0] == Condition::Destroyed {
        let after = run(&mut sim, 60.0);
        assert_eq!(shots_by(&after, team), 0, "no missiles at a burning tank");
    }
}

/// A Red MR squad (1980) dug in in a masonry house at (600, 600); a Blue M1 drives east past it
/// on y = 600 + `offset`. An unaware crew holds fire; an `alert` one has its weapons free and
/// searches for the house (TODO B26, B33).
fn house_ambush(offset: f64, seed: u64, alert: bool) -> Sim {
    let ds = data();
    let m = map_with(&ds, 300, 400, "grassland", |p| {
        p.features.buildings.push(building(1, 594.0, 594.0, 606.0, 606.0, sim_data::BuildingClass::Masonry));
    });
    let blue = vec![unit("1-1", "bulwark_a0", [150.0, 600.0 + offset], 90.0, None)];
    let red = vec![squad("A", "mr_squad", [600.0, 586.0], Some("2-D"), None)];
    let mut sc = scenario(10 * 3600, blue, red);
    sc.date = (1980, 6, 1);
    sc.seed = seed;
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    sim.set_ai_enabled(RED, false);
    let tank = sim.truth().id[0];
    if !alert {
        hold(&mut sim, BLUE, tank);
    }
    // A prepared ambush: the squad is already in the house, dug in, before the tank comes.
    let w = sim.truth_mut();
    w.transform[1].pos = [600.0, 600.0];
    let s = w.squad[1].as_mut().unwrap();
    s.in_building = Some(1);
    s.halted_s = 1.0e6;
    sim.submit_immediate(
        BLUE,
        Command::Move { unit: tank, destination: [1100.0, 600.0 + offset], via: vec![], speed_limit_mps: Some(6.0) },
    );
    sim
}

#[test]
fn an_rpg_team_in_a_house_kills_a_tank_passing_its_flank() {
    // Target A10 and the M8b-b exit (plan §11): a tank passing 100-150 m from an RPG in a house
    // is killed or disabled in more than half of 16 seeds; the rocket waits for the flank and the
    // kill zone (FM 100-2-1 ambush; Grau 1997).
    let mut out = 0;
    let mut early = 0;
    for seed in 1..=16 {
        let offset = 100.0 + (seed % 3) as f64 * 25.0;
        let mut sim = house_ambush(offset, seed, false);
        let squad = sim.truth().id[1];
        for _ in 0..(180 * sim::TICK_HZ) {
            for e in sim.step() {
                // The tank shows its front while the house is within 45 deg of its heading (east).
                let t = sim.truth().transform[0];
                let off = (600.0 - t.pos[1]).atan2(600.0 - t.pos[0]).abs();
                if matches!(e, Event::Fired { unit, .. } if unit == squad) && off < std::f64::consts::FRAC_PI_4 {
                    early += 1;
                }
            }
        }
        if sim.truth().condition[0] != Condition::Operational {
            out += 1;
        }
    }
    println!("A10: tank killed or disabled in {out}/16");
    assert!(out > 8, "tank killed or disabled in {out}/16");
    assert_eq!(early, 0, "no rocket while the tank still shows its front");
}

#[test]
fn an_alert_crew_still_meets_the_house_ambush_often() {
    // TODO B26 after B33 (2026-10-28): the squad is concealed in the room (camouflage 0.5) and
    // holds its rifles at the tank, so its rockets open the ambush. An alert crew (weapons free)
    // still finds the lone house first, at 200-450 m by the commander's eye, and the ambush
    // succeeds in about 3/16; this guards against a regression (it was 1/16).
    let (mut out, mut found_first) = (0, 0);
    for seed in 1..=16 {
        let offset = 100.0 + (seed % 3) as f64 * 25.0;
        let mut sim = house_ambush(offset, seed, true);
        let squad = sim.truth().id[1];
        let mut fired_first = None;
        for _ in 0..(180 * sim::TICK_HZ) {
            for e in sim.step() {
                if let Event::Fired { unit, .. } = e
                    && fired_first.is_none()
                {
                    fired_first = Some(unit != squad);
                }
            }
        }
        if fired_first == Some(true) {
            found_first += 1;
        }
        if sim.truth().condition[0] != Condition::Operational {
            out += 1;
        }
    }
    println!("A10 alert: tank killed or disabled in {out}/16, tank fired first in {found_first}/16");
    assert!(out >= 2, "tank killed or disabled in {out}/16");
}

/// Four Blue M1s (1985) halted 1,500 m from a dug-in Red Fagot team on open ground.
fn platoon_vs_post(seed: u64) -> Sim {
    let ds = data();
    let m = map(&ds, 400, 600, "grassland", |_, _| 0.0);
    let blue = (0..4)
        .map(|i| unit(&format!("1-{}", i + 1), "bulwark_a0", [700.0 + i as f64 * 60.0, 400.0], 90.0, Some("1")))
        .collect();
    let red = vec![squad("AT", "at_team_r", [790.0, 1900.0], Some("2-D"), None)];
    let mut sc = scenario(10 * 3600, blue, red);
    sc.date = (1985, 6, 1);
    sc.seed = seed;
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    sim.set_ai_enabled(RED, false);
    sim.truth_mut().squad[4].as_mut().unwrap().halted_s = 1.0e6;
    sim
}

#[test]
fn a_tank_platoon_puts_out_a_missile_post_once_it_fires() {
    // Plan §5 and §10.8: a missile team fires at a halted tank platoon showing its flank 1,500 m
    // away. The launch gives it away; the tanks make it their first target and engage it with
    // main-gun HE beyond coax reach (ATP 3-20.15 App. F, react to an ATGM). The first missile,
    // fired before anyone knew of the post, often hits; once the tanks fire back the gunner is
    // pinned and later missiles are lost (the 1973 Sagger record, plan §2.2).
    let (mut cleared, mut first_hits, mut later, mut later_hits) = (0, 0, 0, 0);
    for seed in 1..=8 {
        let mut sim = platoon_vs_post(seed);
        let team = sim.truth().id[4];
        let mut results = 0;
        for _ in 0..(300 * sim::TICK_HZ) {
            for e in sim.step() {
                if let Event::ShotResult { unit, hit, .. } = e
                    && unit == team
                {
                    results += 1;
                    match results {
                        1 => first_hits += hit as u32,
                        _ => {
                            later += 1;
                            later_hits += hit as u32;
                        }
                    }
                }
            }
            if !sim.truth().fighting(4) {
                break;
            }
        }
        cleared += !sim.truth().fighting(4) as u32;
    }
    println!("missile post: out in {cleared}/8, first missile hit {first_hits}/8, later {later_hits}/{later}");
    assert!(cleared >= 6, "the post is put out within 5 minutes in {cleared}/8 runs");
    assert!(first_hits >= 2, "the first missile hits at times ({first_hits}/8)");
    assert!(later > 0 && (later_hits as f64) < 0.25 * later as f64, "later missiles mostly lost: {later_hits}/{later}");
}

#[test]
fn a_squad_placed_in_a_house_occupies_it_at_the_start() {
    // Prepared positions (plan §10.8): no walk-in needed, as the ambush demo relies on.
    let ds = data();
    let m = map_with(&ds, 200, 200, "grassland", |p| {
        p.features.buildings.push(building(1, 394.0, 394.0, 406.0, 406.0, sim_data::BuildingClass::Masonry));
    });
    let red = vec![
        squad("A", "mr_squad", [400.0, 400.0], Some("2-D"), None),
        squad("B", "mr_squad", [300.0, 300.0], Some("2-D"), None),
    ];
    let sim = Sim::new(&scenario(10 * 3600, vec![], red), ds, m).unwrap();
    let w = sim.truth();
    assert_eq!(w.squad[0].as_ref().unwrap().in_building, Some(1));
    assert_eq!(w.squad[1].as_ref().unwrap().in_building, None);
}
