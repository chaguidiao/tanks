//! Phase 2 (pre-M7e schedule): commander AI tactics. Alternate positions and displacement on a
//! trigger, the reserve counter-attack, reconnaissance releasing the main body, and the
//! synchronised assault from an assault position. Scenarios are small and synthetic; side-effects
//! of shooting are avoided with HOLD FIRE so the triggers under test are the only moving part.

mod common;

use ai::commander::Phase;
use common::*;
use sim::view::{Condition, WeaponsStatus};
use sim::{Addressee, Command, SideId, Sim};
use sim_data::{AiPlan, AiTask, BattlePosition, Mission, ObjectiveDef};

const BLUE: SideId = SideId(0);
const RED: SideId = SideId(1);

fn task(platoons: &[&str], mission: Mission) -> AiTask {
    AiTask {
        platoons: platoons.iter().map(|s| s.to_string()).collect(),
        mission,
        start_s: 0.0,
        release_on_contact: false,
        latest_s: None,
    }
}

fn hold_fire(sim: &mut Sim, side: SideId) {
    let ids: Vec<_> = sim.perceived(side).own.iter().map(|u| u.id).collect();
    for id in ids {
        sim.submit_immediate(side, Command::WeaponsControl { to: Addressee::Unit(id), status: WeaponsStatus::Hold });
    }
}

/// Centroid of a platoon's operational vehicles, from the truth (tests may look).
fn centroid(sim: &Sim, side: SideId, callsign: &str) -> [f64; 2] {
    let pic = sim.perceived(side);
    let pl = pic.platoons.iter().find(|p| p.callsign == callsign).expect("platoon");
    let pos: Vec<_> = pl.members.iter().filter_map(|m| pic.unit(*m)).map(|u| u.pos).collect();
    let n = pos.len() as f64;
    [pos.iter().map(|p| p[0]).sum::<f64>() / n, pos.iter().map(|p| p[1]).sum::<f64>() / n]
}

fn blue_platoon(n: usize, at: [f64; 2], name: &str) -> Vec<sim_data::UnitDef> {
    (0..n)
        .map(|i| unit(&format!("{name}-{}", i + 1), "bulwark_a1", [at[0] + i as f64 * 90.0, at[1]], 0.0, Some(name)))
        .collect()
}

const PRIMARY: [f64; 2] = [1000.0, 1500.0];
const ALTERNATE: [f64; 2] = [1000.0, 900.0];

fn defend_with_alternate() -> Mission {
    Mission::Defend {
        centre: PRIMARY,
        radius_m: 220.0,
        facing_deg: 0.0,
        alternates: vec![BattlePosition { centre: ALTERNATE, radius_m: 220.0, facing_deg: 0.0 }],
    }
}

/// Blue defends `PRIMARY` with an alternate to the south; `red` units are added to the scenario.
fn defence_scenario(red: Vec<sim_data::UnitDef>) -> Sim {
    let ds = data();
    let m = map(&ds, 500, 1000, "cropland", |_, _| 0.0);
    let blue = blue_platoon(4, [PRIMARY[0] - 135.0, PRIMARY[1] - 100.0], "1");
    let mut sc = scenario(10 * 3600, blue, red);
    sc.sides[0].ai = Some(AiPlan { tasks: vec![task(&["1"], defend_with_alternate())] });
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    hold_fire(&mut sim, BLUE);
    hold_fire(&mut sim, RED);
    sim
}

#[test]
fn defenders_stay_in_the_primary_position_when_nothing_triggers() {
    let mut sim = defence_scenario(vec![unit("R1", "kurgan_b", [1000.0, 3800.0], 180.0, None)]);
    run(&mut sim, 300.0);
    let c = centroid(&sim, BLUE, "1");
    assert!(dist(c, PRIMARY) < 250.0, "{c:?}");
}

#[test]
fn defenders_displace_to_the_alternate_when_the_enemy_closes_on_the_position() {
    // ATP 3-20.15 ch. 5: displace on the trigger. The tank sits 900 m north of the primary position; with the enemy far away (the test above)
    // the same platoon stays put, so the contact is what triggers the move.
    let mut sim = defence_scenario(vec![unit("R1", "kurgan_b", [1000.0, 2400.0], 180.0, None)]);
    run(&mut sim, 300.0);
    let c = centroid(&sim, BLUE, "1");
    assert!(dist(c, ALTERNATE) < 250.0, "on the alternate position: {c:?}");
}

#[test]
fn defenders_displace_after_losses_and_only_once_per_loss() {
    let mut sim = defence_scenario(vec![unit("R1", "kurgan_b", [1000.0, 3800.0], 180.0, None)]);
    run(&mut sim, 60.0);
    // Lose one of four vehicles: exactly the displacement threshold of the doctrine (25 %).
    let victims: Vec<usize> = (0..sim.truth().id.len()).filter(|&k| sim.truth().side[k] == BLUE).take(1).collect();
    for k in victims {
        sim.truth_mut().condition[k] = Condition::Destroyed;
    }
    run(&mut sim, 240.0);
    let c = centroid(&sim, BLUE, "1");
    assert!(dist(c, ALTERNATE) < 250.0, "{c:?}");
    // No further alternate exists, and the same losses do not trigger a second move.
    run(&mut sim, 200.0);
    assert!(dist(centroid(&sim, BLUE, "1"), ALTERNATE) < 250.0);
}

fn reserve_scenario(red: Vec<sim_data::UnitDef>) -> Sim {
    let ds = data();
    let m = map(&ds, 500, 1000, "cropland", |_, _| 0.0);
    let blue = blue_platoon(3, [900.0, 450.0], "2");
    let mut sc = scenario(10 * 3600, blue, red);
    sc.objectives = vec![ObjectiveDef { name: "OBJ".into(), centre: [1000.0, 1800.0], radius_m: 250.0 }];
    sc.sides[0].ai = Some(AiPlan {
        tasks: vec![task(
            &["2"],
            Mission::Reserve {
                assembly: BattlePosition { centre: [1000.0, 500.0], radius_m: 150.0, facing_deg: 0.0 },
                objective: "OBJ".into(),
            },
        )],
    });
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    hold_fire(&mut sim, BLUE);
    hold_fire(&mut sim, RED);
    sim
}

#[test]
fn the_reserve_holds_its_assembly_area_until_the_objective_is_contested() {
    let mut sim = reserve_scenario(vec![unit("R1", "kurgan_b", [1000.0, 3900.0], 180.0, None)]);
    run(&mut sim, 240.0);
    assert!(dist(centroid(&sim, BLUE, "2"), [1000.0, 500.0]) < 250.0);
    assert_eq!(sim.commander_phases(BLUE), vec![Phase::Reserve]);
}

#[test]
fn the_reserve_counter_attacks_when_the_enemy_reaches_the_objective() {
    // ATP 3-90.5 / FM 3-90: the reserve is committed at the decisive point. The enemy tank is
    // 300 m short of the objective centre.
    let mut sim = reserve_scenario(vec![unit("R1", "kurgan_b", [1000.0, 2100.0], 180.0, None)]);
    let mut phases = Vec::new();
    for _ in 0..300 {
        run(&mut sim, 1.0);
        let p = sim.commander_phases(BLUE)[0];
        if phases.last() != Some(&p) {
            phases.push(p);
        }
    }
    assert_eq!(phases[0], Phase::Reserve);
    assert!(phases.len() > 1 && !phases.contains(&Phase::BrokenOff), "committed as an attack: {phases:?}");
    let c = centroid(&sim, BLUE, "2");
    assert!(c[1] > 1100.0, "moved forward toward the objective: {c:?}");
}

#[test]
fn the_reserve_is_committed_by_losses_elsewhere() {
    let ds = data();
    let m = map(&ds, 500, 1000, "cropland", |_, _| 0.0);
    let mut blue = blue_platoon(3, [900.0, 450.0], "2");
    blue.extend(blue_platoon(3, [900.0, 1300.0], "1"));
    let red = vec![unit("R1", "kurgan_b", [1000.0, 3900.0], 180.0, None)];
    let mut sc = scenario(10 * 3600, blue, red);
    sc.objectives = vec![ObjectiveDef { name: "OBJ".into(), centre: [1000.0, 1800.0], radius_m: 250.0 }];
    sc.sides[0].ai = Some(AiPlan {
        tasks: vec![
            task(
                &["2"],
                Mission::Reserve {
                    assembly: BattlePosition { centre: [1000.0, 500.0], radius_m: 150.0, facing_deg: 0.0 },
                    objective: "OBJ".into(),
                },
            ),
            task(
                &["1"],
                Mission::Defend { centre: [1000.0, 1300.0], radius_m: 200.0, facing_deg: 0.0, alternates: vec![] },
            ),
        ],
    });
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    hold_fire(&mut sim, BLUE);
    hold_fire(&mut sim, RED);
    run(&mut sim, 120.0);
    assert_eq!(sim.commander_phases(BLUE)[0], Phase::Reserve);
    // Two of six vehicles destroyed: a third of the side, over the commit threshold (25 %).
    let victims: Vec<usize> = (0..sim.truth().id.len())
        .filter(|&k| sim.truth().side[k] == BLUE && sim.truth().callsign[k].starts_with("1-"))
        .take(2)
        .collect();
    for k in victims {
        sim.truth_mut().condition[k] = Condition::Destroyed;
    }
    run(&mut sim, 60.0);
    assert_ne!(sim.commander_phases(BLUE)[0], Phase::Reserve, "committed");
}

/// Red: a recon platoon and a main-body attack platoon waiting behind it; Blue sits on the
/// objective and never fires, so Red's picture changes only because the recon sees it.
fn recon_scenario(release_on_contact: bool, latest_s: Option<f64>, with_recon: bool) -> Sim {
    let ds = data();
    let m = map(&ds, 500, 2000, "cropland", |_, _| 0.0);
    let blue = blue_platoon(2, [900.0, 1000.0], "1");
    let mut red: Vec<_> = (0..3)
        .map(|i| unit(&format!("21{}", i + 1), "kurgan_b", [900.0 + i as f64 * 80.0, 7600.0], 180.0, Some("21")))
        .collect();
    red.extend(
        (0..2)
            .map(|i| unit(&format!("31{}", i + 1), "falcon_r1", [900.0 + i as f64 * 80.0, 7300.0], 180.0, Some("31"))),
    );
    let mut sc = scenario(10 * 3600, blue, red);
    sc.objectives = vec![ObjectiveDef { name: "OBJ".into(), centre: [1000.0, 1000.0], radius_m: 300.0 }];
    let mut tasks = Vec::new();
    if with_recon {
        tasks.push(task(
            &["31"],
            Mission::Recon {
                axis: vec![[1000.0, 5500.0], [1000.0, 4000.0]],
                observe: BattlePosition { centre: [1000.0, 3200.0], radius_m: 120.0, facing_deg: 180.0 },
            },
        ));
    }
    let mut attack = task(&["21"], Mission::Attack { axis: vec![[1000.0, 6000.0]], objective: "OBJ".into() });
    attack.release_on_contact = release_on_contact;
    attack.latest_s = latest_s;
    tasks.push(attack);
    sc.sides[1].ai = Some(AiPlan { tasks });
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    hold_fire(&mut sim, BLUE);
    sim
}

#[test]
fn reconnaissance_releases_the_main_body_on_the_first_contact() {
    // FM 3-98: gain contact with the reconnaissance element, then move the main body.
    let mut sim = recon_scenario(true, Some(3000.0), true);
    let start = centroid(&sim, RED, "21");
    let mut first_contact = None;
    let mut released_at = None;
    let mut recon_phase = Vec::new();
    for s in 0..900 {
        run(&mut sim, 1.0);
        let phases = sim.commander_phases(RED);
        if recon_phase.last() != Some(&phases[0]) {
            recon_phase.push(phases[0]);
        }
        if first_contact.is_none() && sim.perceived(RED).live_contacts().next().is_some() {
            first_contact = Some(s);
        }
        if released_at.is_none() && phases[1] != Phase::Waiting {
            released_at = Some(s);
        } else if released_at.is_none() {
            assert!(dist(centroid(&sim, RED, "21"), start) < 60.0, "main body waits at t={s}");
        }
    }
    let (c, r) = (first_contact.expect("recon found Blue"), released_at.expect("the main body was released"));
    // The commander decides every 5 s; the release follows the first contact, never precedes it.
    assert!(r >= c && r - c <= 10, "contact at {c} s, release at {r} s");
    assert!(c > 20 && c < 700, "after the recon moved out: {c} s");
    assert!(recon_phase.iter().any(|p| matches!(p, Phase::March { .. })), "{recon_phase:?}");
    assert_eq!(*recon_phase.last().unwrap(), Phase::Observing, "recon stops to observe: {recon_phase:?}");
    let rc = centroid(&sim, RED, "31");
    assert!(rc[1] < 7000.0 && rc[1] > 2500.0, "stopped short of the enemy: {rc:?}");
}

#[test]
fn a_task_waiting_for_contact_goes_at_its_latest_time() {
    let mut sim = recon_scenario(true, Some(100.0), false);
    run(&mut sim, 95.0);
    assert_eq!(sim.commander_phases(RED)[0], Phase::Waiting);
    assert!(sim.perceived(RED).live_contacts().next().is_none(), "no contact in the first 95 s");
    run(&mut sim, 20.0);
    assert!(matches!(sim.commander_phases(RED)[0], Phase::March { .. }), "{:?}", sim.commander_phases(RED));
}

#[test]
fn a_task_without_a_release_condition_starts_at_once() {
    let mut sim = recon_scenario(false, None, false);
    run(&mut sim, 12.0);
    assert!(matches!(sim.commander_phases(RED)[0], Phase::March { .. }));
}

#[test]
fn platoons_close_up_on_the_assault_position_and_assault_together() {
    // The axis ends 5 km short of the objective: the march must carry on to the objective anyway.
    // FM 3-90 ch. 3: the attack position synchronises the assault. Platoon 22 trails 500 m
    // behind platoon 21; both must be at the assault position (3 km) before the shake-out.
    let ds = data();
    let m = map(&ds, 500, 2000, "cropland", |_, _| 0.0);
    let blue = blue_platoon(1, [1000.0, 300.0], "1");
    let mut red: Vec<_> = (0..3)
        .map(|i| unit(&format!("21{}", i + 1), "kurgan_b", [900.0 + i as f64 * 80.0, 7000.0], 180.0, Some("21")))
        .collect();
    red.extend(
        (0..3).map(|i| unit(&format!("22{}", i + 1), "kurgan_b", [900.0 + i as f64 * 80.0, 7500.0], 180.0, Some("22"))),
    );
    let mut sc = scenario(10 * 3600, blue, red);
    sc.objectives = vec![ObjectiveDef { name: "OBJ".into(), centre: [1000.0, 1000.0], radius_m: 300.0 }];
    sc.sides[1].ai = Some(AiPlan {
        tasks: vec![task(&["21", "22"], Mission::Attack { axis: vec![[1000.0, 6000.0]], objective: "OBJ".into() })],
    });
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    hold_fire(&mut sim, BLUE);
    let mut seen = Vec::new();
    let mut at_battle = None;
    for _ in 0..1500 {
        run(&mut sim, 1.0);
        let p = sim.commander_phases(RED)[0];
        if seen.last() != Some(&p) {
            seen.push(p);
        }
        if p == Phase::Battle && at_battle.is_none() {
            at_battle = Some((centroid(&sim, RED, "21"), centroid(&sim, RED, "22")));
        }
        if p == Phase::Consolidate {
            break;
        }
    }
    assert!(seen.contains(&Phase::PreBattle) && seen.contains(&Phase::Battle), "{seen:?}");
    let (a, b) = at_battle.expect("assault began");
    let obj = [1000.0, 1000.0];
    // Both platoons were at (or inside) the assault position when the assault began.
    assert!(dist(a, obj) < 3000.0 + 300.0, "21 at {:.0} m", dist(a, obj));
    assert!(dist(b, obj) < 3000.0 + 300.0, "22 at {:.0} m", dist(b, obj));
    assert!((dist(a, obj) - dist(b, obj)).abs() < 300.0, "abreast: {:.0} vs {:.0}", dist(a, obj), dist(b, obj));
}
