//! M8f1 infantry AI (docs/plans/M8F1-INFANTRY-AI.md): react to ambush (near) with its ON AMBUSH
//! settings and the AMBUSHED branch condition.

mod common;

use ai::picture::{AmbushReaction, Branch, ConditionSet, Plan, PlanStep, PlatoonId, TriggerCondition, WeaponsStatus};
use common::*;
use sim::{Addressee, Command, Event, PlatoonReport, SideId, Sim};

const BLUE: SideId = SideId(0);
const RED: SideId = SideId(1);
const P1: PlatoonId = PlatoonId(1);

/// A Red MR squad (1980, RPG-7 class) dug in in a masonry house at (600, 600); a Blue platoon of
/// two M1s drives east on y = 720 past it. The crews hold fire, so the rocket opens the
/// ambush.
fn ambush_sim(reaction: AmbushReaction, seed: u64) -> Sim {
    let ds = data();
    let m = map_with(&ds, 400, 400, "grassland", |p| {
        p.features.buildings.push(building(1, 594.0, 594.0, 606.0, 606.0, sim_data::BuildingClass::Masonry));
    });
    let blue = vec![
        unit("1-1", "bulwark_a0", [200.0, 720.0], 90.0, Some("1")),
        unit("1-2", "bulwark_a0", [150.0, 740.0], 90.0, Some("1")),
    ];
    let red = vec![squad("A", "mr_squad", [600.0, 600.0], Some("2-D"), None)];
    let mut sc = scenario(10 * 3600, blue, red);
    sc.date = (1980, 6, 1);
    sc.seed = seed;
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    sim.set_ai_enabled(RED, false);
    sim.submit_immediate(BLUE, Command::WeaponsControl { to: Addressee::Platoon(P1), status: WeaponsStatus::Hold });
    let s = sim.truth_mut().squad[2].as_mut().unwrap();
    s.halted_s = 1.0e6;
    sim.submit_immediate(BLUE, Command::SetAmbushReaction { platoon: P1, reaction });
    sim
}

fn move_east(sim: &mut Sim) {
    sim.submit_immediate(
        BLUE,
        Command::PlatoonMove {
            platoon: P1,
            destination: [1300.0, 720.0],
            via: vec![],
            formation: Some(sim_data::Formation::Column),
            technique: sim_data::Technique::Traveling,
            speed_limit_mps: Some(6.0),
        },
    );
}

/// Run until the platoon reports the ambush; then `after` seconds more. Returns the leader's x
/// at the ambush and after, and the events from the ambush on.
fn through_ambush(sim: &mut Sim, after: f64) -> Option<(f64, f64, Vec<Event>)> {
    for _ in 0..(150 * sim::TICK_HZ) {
        let ev = sim.step();
        if ev.iter().any(|e| matches!(e, Event::PlatoonReport { report: PlatoonReport::Ambush { .. }, .. })) {
            let x0 = sim.truth().transform[0].pos[0];
            let ev = ev.into_iter().chain(run(sim, after)).collect();
            return Some((x0, sim.truth().transform[0].pos[0], ev));
        }
    }
    None
}

#[test]
fn ambushed_tanks_back_out_of_the_kill_zone_by_default() {
    let mut sim = ambush_sim(AmbushReaction::default(), 1);
    move_east(&mut sim);
    let (x0, x1, ev) = through_ambush(&mut sim, 30.0).expect("the squad fires on the platoon");
    let w = sim.truth();
    let house = [600.0, 600.0];
    assert!(x1 < x0 - 60.0, "reversed from x {x0:.0} to {x1:.0}");
    assert!(dist(w.transform[0].pos, house) > 250.0, "out of the kill zone");
    assert!(ev.iter().any(|e| matches!(e, Event::SmokeLaunched { auto: false, .. })), "screened with smoke");
    // Backing out keeps the front to the house.
    let to_house = ai::drive::bearing(w.transform[0].pos, house);
    let off = (w.transform[0].heading - to_house + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU)
        - std::f64::consts::PI;
    assert!(off.abs() < 0.8, "front to the shooter (off by {:.0} deg)", off.to_degrees());
}

#[test]
fn drive_through_keeps_the_platoon_moving() {
    // The trailing tank (the leader may be hit in the kill zone, which is the risk of this SOP).
    let mut sim = ambush_sim(AmbushReaction::DriveThrough, 1);
    move_east(&mut sim);
    through_ambush(&mut sim, 0.0).expect("the squad fires on the platoon");
    let x0 = sim.truth().transform[1].pos[0];
    run(&mut sim, 25.0);
    let x1 = sim.truth().transform[1].pos[0];
    assert!(x1 > x0 + 100.0, "drove on from x {x0:.0} to {x1:.0}");
}

#[test]
fn halt_leaves_it_to_the_contact_drill() {
    let mut sim = ambush_sim(AmbushReaction::Halt, 1);
    move_east(&mut sim);
    let (x0, x1, _) = through_ambush(&mut sim, 30.0).expect("the squad fires on the platoon");
    assert!((x1 - x0).abs() < 40.0, "halted near x {x0:.0} ({x1:.0})");
}

#[test]
fn an_ambushed_branch_takes_the_platoon_to_its_alternate_route() {
    // CP 1 east past the house, with IF ON LEG: AMBUSHED, go to CP 2 (south, away from it).
    let mut sim = ambush_sim(AmbushReaction::Halt, 1);
    let mut cp1 = PlanStep::new(1, [1300.0, 720.0]);
    cp1.branch = Some(Branch {
        when: ConditionSet { all: true, conditions: vec![TriggerCondition::Ambushed { platoon: P1 }] },
        goto: 2,
    });
    let plan = Plan { steps: vec![cp1, PlanStep::new(2, [300.0, 1200.0])] };
    sim.submit_immediate(BLUE, Command::SetPlan { platoon: P1, plan, start: 1 });
    let (_, _, ev) = through_ambush(&mut sim, 60.0).expect("the squad fires on the platoon");
    assert!(ev.iter().any(|e| matches!(e, Event::PlatoonReport { report: PlatoonReport::PlanBranch { to: 2 }, .. })));
    let y = sim.truth().transform[0].pos[1];
    assert!(y > 800.0, "heading for CP 2 at once (y {y:.0})");
    let v = sim.perceived(BLUE);
    assert!(v.platoon(P1).unwrap().ambushed_tick.is_some());
}
