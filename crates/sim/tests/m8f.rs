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

/// A hamlet of nine houses across the straight route from (300, 800) to (2100, 800); a Blue tank
/// platoon (and, if `squad`, a friendly squad on the ground beside it) west of it.
fn hamlet_sim(squad: bool) -> Sim {
    let ds = data();
    let m = map_with(&ds, 600, 400, "grassland", |p| {
        let mut id = 1;
        for i in 0..3 {
            for j in 0..3 {
                let (x, y) = (1120.0 + i as f64 * 60.0, 720.0 + j as f64 * 60.0);
                p.features.buildings.push(building(id, x, y, x + 14.0, y + 10.0, sim_data::BuildingClass::Masonry));
                id += 1;
            }
        }
    });
    let mut blue = vec![
        unit("1-1", "bulwark_a1", [300.0, 800.0], 90.0, Some("1")),
        unit("1-2", "bulwark_a1", [250.0, 820.0], 90.0, Some("1")),
    ];
    if squad {
        blue.push(squad_unit());
    }
    let red = vec![unit("211", "kurgan_b", [2300.0, 3000.0], 180.0, Some("21"))];
    let mut sim = Sim::new(&scenario(10 * 3600, blue, red), ds, m).unwrap();
    sim.set_ai_enabled(RED, false);
    sim
}

fn squad_unit() -> sim_data::UnitDef {
    squad("1-D1", "rifle_squad_b", [320.0, 760.0], Some("1-D"), None)
}

/// Largest distance of the platoon's planned route from the straight line y = 800.
fn detour(sim: &mut Sim) -> f64 {
    move_east_to(sim, [2100.0, 800.0]);
    run(sim, 3.0);
    let v = sim.perceived(BLUE);
    let r = &v.platoon(P1).unwrap().route;
    assert!(!r.is_empty(), "a route is planned");
    r.iter().map(|p| (p[1] - 800.0).abs()).fold(0.0, f64::max)
}

fn move_east_to(sim: &mut Sim, to: [f64; 2]) {
    sim.submit_immediate(
        BLUE,
        Command::PlatoonMove {
            platoon: P1,
            destination: to,
            via: vec![],
            formation: None,
            technique: sim_data::Technique::Traveling,
            speed_limit_mps: None,
        },
    );
}

#[test]
fn tank_platoons_route_round_built_up_ground_by_default() {
    let mut sim = hamlet_sim(false);
    let d = detour(&mut sim);
    // Houses span y 720-850; with the 30 m margin the route passes beyond 690 or 880.
    assert!(d > 75.0, "goes round the hamlet ({d:.0} m off the line)");
}

#[test]
fn built_up_enter_goes_through() {
    let mut sim = hamlet_sim(false);
    sim.submit_immediate(BLUE, Command::SetBuiltUp { platoon: P1, built_up: ai::picture::BuiltUp::Enter });
    let d = detour(&mut sim);
    assert!(d < 60.0, "straight through ({d:.0} m off the line)");
}

#[test]
fn squads_on_the_ground_let_the_tanks_enter() {
    let mut sim = hamlet_sim(true);
    let d = detour(&mut sim);
    assert!(d < 60.0, "with dismounts close by, straight through ({d:.0} m off the line)");
}

// --- commander dismount points (plan §4) ----------------------------------------------------

fn ai_task(platoons: &[&str], mission: sim_data::Mission) -> sim_data::AiTask {
    sim_data::AiTask {
        platoons: platoons.iter().map(|s| s.to_string()).collect(),
        mission,
        start_s: 0.0,
        release_on_contact: false,
        latest_s: None,
    }
}

/// Red BMP-2 class platoon of three, each with a motor rifle squad, attacks objective OBJ at
/// (1000, 2600) from the south; Blue is a passive tank far away.
fn bmp_attack() -> Sim {
    let ds = data();
    let m = map(&ds, 500, 900, "grassland", |_, _| 0.0);
    let mut red = Vec::new();
    for i in 0..3 {
        let cs = format!("22{}", i + 1);
        red.push(unit(&cs, "skif_m1", [880.0 + i as f64 * 60.0, 400.0], 0.0, Some("22")));
    }
    for i in 0..3 {
        red.push(squad(&format!("22{}A", i + 1), "mr_squad", [0.0, 0.0], None, Some(&format!("22{}", i + 1))));
    }
    let blue = vec![unit("1-1", "bulwark_a1", [1900.0, 3400.0], 180.0, Some("1"))];
    let mut sc = scenario(10 * 3600, blue, red);
    sc.objectives = vec![sim_data::ObjectiveDef { name: "OBJ".into(), centre: [1000.0, 2600.0], radius_m: 200.0 }];
    sc.sides[1].ai = Some(sim_data::AiPlan {
        tasks: vec![ai_task(&["22"], sim_data::Mission::Attack { axis: vec![], objective: "OBJ".into() })],
    });
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    sim.set_ai_enabled(BLUE, false);
    let tank = sim.truth().id[0];
    sim.submit_immediate(BLUE, Command::WeaponsControl { to: Addressee::Unit(tank), status: WeaponsStatus::Hold });
    sim
}

#[test]
fn commander_ifvs_dismount_short_of_the_objective_and_overwatch() {
    // generic-opfor: dismount 400 m short of the objective's edge (FM 100-2-1 ch. 5).
    let mut sim = bmp_attack();
    // Entity 0 is Blue's tank; Red's carriers are 1-3, their squads 4-6.
    let squads = [4usize, 5, 6];
    let mut dismounted_at = None;
    for _ in 0..(1500 / 5) {
        run(&mut sim, 5.0);
        let w = sim.truth();
        if dismounted_at.is_none() && squads.iter().any(|&k| !w.mounted(k)) {
            dismounted_at = Some(dist(w.transform[1].pos, [1000.0, 2600.0]));
        }
    }
    let d = dismounted_at.expect("the squads dismount");
    assert!((450.0..=750.0).contains(&d), "dismounted {d:.0} m from the objective centre");
    let w = sim.truth();
    for &k in &squads {
        let r = dist(w.transform[k].pos, [1000.0, 2600.0]);
        assert!(!w.mounted(k) && r < 260.0, "squad {k} on foot at the objective ({r:.0} m)");
    }
    // The carriers overwatched from the dismount point, then moved up once the squads held it.
    for c in [1, 2, 3] {
        let r = dist(w.transform[c].pos, [1000.0, 2600.0]);
        assert!(r < 300.0, "carrier {c} consolidated on the objective ({r:.0} m)");
    }
}

/// Blue M2 platoon of two with squads defends a battle position at (1000, 1500) facing north;
/// a masonry house 60 m in front of it.
fn m2_defence() -> Sim {
    let ds = data();
    let m = map_with(&ds, 500, 900, "grassland", |p| {
        // Two houses in front of the position, one behind it.
        p.features.buildings.push(building(1, 990.0, 1570.0, 1010.0, 1584.0, sim_data::BuildingClass::Masonry));
        p.features.buildings.push(building(2, 1100.0, 1560.0, 1120.0, 1574.0, sim_data::BuildingClass::Masonry));
        p.features.buildings.push(building(3, 1000.0, 1380.0, 1020.0, 1394.0, sim_data::BuildingClass::Masonry));
    });
    let blue = vec![
        unit("1-1", "ward_m2", [900.0, 600.0], 0.0, Some("1")),
        unit("1-2", "ward_m2", [980.0, 600.0], 0.0, Some("1")),
        squad("1-1A", "rifle_squad_b", [0.0, 0.0], None, Some("1-1")),
        squad("1-2A", "rifle_squad_b", [0.0, 0.0], None, Some("1-2")),
    ];
    let red = vec![unit("211", "kurgan_b", [1000.0, 3400.0], 180.0, Some("21"))];
    let mut sc = scenario(10 * 3600, blue, red);
    sc.sides[0].ai = Some(sim_data::AiPlan {
        tasks: vec![ai_task(
            &["1"],
            sim_data::Mission::Defend {
                centre: [1000.0, 1500.0],
                radius_m: 150.0,
                facing_deg: 0.0,
                alternates: vec![],
            },
        )],
    });
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    sim.set_ai_enabled(RED, false);
    sim
}

#[test]
fn commander_ifvs_defend_with_their_squads_out_in_front() {
    let mut sim = m2_defence();
    run(&mut sim, 900.0);
    let w = sim.truth();
    for c in [0, 1] {
        assert!(dist(w.transform[c].pos, [1000.0, 1500.0]) < 250.0, "carrier {c} in the battle position");
    }
    for k in [2, 3] {
        assert!(!w.mounted(k), "squad {k} dismounted");
        let y = w.transform[k].pos[1];
        assert!(y > 1500.0, "squad {k} forward of the position (y {y:.0})");
    }
    // One squad to each house that covers the approach (plan §5), none in the one behind.
    let mut houses: Vec<Option<u32>> = [2, 3].iter().map(|&k| w.squad[k].as_ref().unwrap().in_building).collect();
    houses.sort();
    assert_eq!(houses, vec![Some(1), Some(2)]);
}

/// A Red missile team (1985, Fagot class) in a masonry house at (600, 600) with a second house
/// 130 m behind it; two Blue M1s halted 1,400 m north, side-on to it and holding fire.
fn missile_post() -> Sim {
    let ds = data();
    let m = map_with(&ds, 300, 600, "grassland", |p| {
        p.features.buildings.push(building(1, 594.0, 594.0, 606.0, 606.0, sim_data::BuildingClass::Masonry));
        p.features.buildings.push(building(2, 594.0, 464.0, 606.0, 476.0, sim_data::BuildingClass::Masonry));
    });
    let blue = vec![
        unit("1-1", "bulwark_a0", [560.0, 2000.0], 90.0, Some("1")),
        unit("1-2", "bulwark_a0", [660.0, 2000.0], 90.0, Some("1")),
    ];
    let red = vec![squad("AT", "at_team_r", [600.0, 600.0], Some("2-AT"), None)];
    let mut sc = scenario(10 * 3600, blue, red);
    sc.date = (1985, 6, 1);
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    sim.set_ai_enabled(RED, false);
    sim.submit_immediate(BLUE, Command::WeaponsControl { to: Addressee::Platoon(P1), status: WeaponsStatus::Hold });
    sim
}

#[test]
fn a_missile_team_moves_house_after_two_shots() {
    let mut sim = missile_post();
    let team = sim.truth().id[2];
    let mut shots_from_1 = 0;
    let mut moved = false;
    for _ in 0..(600 * sim::TICK_HZ) {
        for e in sim.step() {
            if matches!(e, Event::Fired { unit, .. } if unit == team)
                && sim.truth().squad[2].as_ref().unwrap().in_building == Some(1)
            {
                shots_from_1 += 1;
            }
        }
        if sim.truth().squad[2].as_ref().unwrap().in_building == Some(2) {
            moved = true;
            break;
        }
    }
    assert!(moved, "the team moved to the second house ({shots_from_1} shots from the first)");
    assert_eq!(shots_from_1, 2, "after two missiles from the first");
}
