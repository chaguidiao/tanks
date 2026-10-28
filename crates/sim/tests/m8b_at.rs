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
