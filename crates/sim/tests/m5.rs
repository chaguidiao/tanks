//! M5 acceptance: a complete scenario played start to finish by both commander AIs.

mod common;

use ai::commander::Phase;
use common::*;
use sim::view::Condition;
use sim::{EndReason, Event, SideId, Sim};
use sim_data::{AiPlan, AiTask, Mission, ObjectiveDef, VictoryDef};

#[test]
fn opfor_attack_against_a_defended_ridge_plays_to_endex() {
    let ds = data();
    // 2 km x 7 km; low ridge north of the objective; Red enters from the north.
    let m = map(&ds, 500, 1750, "cropland", |_, y| 10.0 * (-((y - 1500.0) / 160.0).powi(2)).exp());
    let blue: Vec<_> = (0..4)
        .map(|i| unit(&format!("1-{}", i + 1), "bulwark_a1", [700.0 + i as f64 * 120.0, 700.0], 0.0, Some("1")))
        .collect();
    let mut red = Vec::new();
    for p in 0..3 {
        for v in 0..3 {
            red.push(unit(
                &format!("2{}{}", p + 1, v + 1),
                "kurgan_b",
                [500.0 + p as f64 * 500.0 + v as f64 * 60.0, 6700.0 - v as f64 * 60.0],
                180.0,
                Some(&format!("2{}", p + 1)),
            ));
        }
    }
    let mut sc = scenario(10 * 3600, blue, red);
    sc.objectives = vec![ObjectiveDef { name: "OBJ".into(), centre: [1000.0, 1200.0], radius_m: 300.0 }];
    sc.victory =
        Some(VictoryDef { time_limit_s: 2400.0, objective: Some("OBJ".into()), attacker: 1, destroyed_fraction: 0.7 });
    sc.sides[0].ai = Some(AiPlan {
        tasks: vec![AiTask {
            platoons: vec!["1".into()],
            mission: Mission::Defend { centre: [1000.0, 1420.0], radius_m: 250.0, facing_deg: 0.0 },
            start_s: 0.0,
        }],
    });
    sc.sides[1].ai = Some(AiPlan {
        tasks: vec![AiTask {
            platoons: vec!["21".into(), "22".into(), "23".into()],
            mission: Mission::Attack {
                axis: vec![[1000.0, 5600.0], [1000.0, 4000.0], [1000.0, 2600.0]],
                objective: "OBJ".into(),
            },
            start_s: 30.0,
        }],
    });
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    let mut phases = Vec::new();
    let mut end = None;
    for _ in 0..2400 {
        for e in run(&mut sim, 1.0) {
            if let Event::ScenarioEnd { winner, reason } = e {
                end = Some((winner, reason));
            }
        }
        let p = sim.commander_phases(SideId(1));
        if let Some(&ph) = p.first()
            && phases.last() != Some(&ph)
        {
            phases.push(ph);
        }
        if end.is_some() {
            break;
        }
    }
    let (winner, reason) = end.expect("scenario must reach ENDEX within the time limit");
    assert!(phases.iter().any(|p| matches!(p, Phase::March { .. })), "{phases:?}");
    assert!(phases.contains(&Phase::Battle), "deployed to battle formation: {phases:?}");
    let w = sim.truth();
    let red_lost = (4..13).filter(|&k| w.condition[k] != Condition::Operational).count();
    assert!(red_lost >= 3, "the defence inflicts losses ({red_lost})");
    // Hull-down, thermal-equipped defenders with the better gun should hold.
    assert_eq!(winner, Some(SideId(0)), "{reason:?}; phases {phases:?}");
    assert!(matches!(reason, EndReason::ForceDestroyed { side: SideId(1) } | EndReason::TimeLimit { .. }));
}
