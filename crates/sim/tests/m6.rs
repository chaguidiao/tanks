//! M6: replays reproduce games exactly; triggers release contingency orders.

mod common;

use ai::picture::{Graphic, GraphicKind, PlatoonId, TriggerCondition, WeaponsStatus};
use common::*;
use sim::{Addressee, Command, EntityId, Event, Replay, SideId, Sim};
use sim_data::{AiPlan, AiTask, Mission, ObjectiveDef, Technique};

const BLUE: SideId = SideId(0);
const RED: SideId = SideId(1);

fn setup() -> (std::sync::Arc<sim_data::DataSet>, std::sync::Arc<terrain::BakedMap>, sim_data::ScenarioDef) {
    let ds = data();
    let m = map(&ds, 400, 1200, "cropland", |_, y| 6.0 * (-((y - 1400.0) / 150.0).powi(2)).exp());
    let blue: Vec<_> = (0..4)
        .map(|i| unit(&format!("1-{}", i + 1), "bulwark_a1", [600.0 + i as f64 * 100.0, 800.0], 0.0, Some("1")))
        .collect();
    let red: Vec<_> = (0..3)
        .map(|i| unit(&format!("21{}", i + 1), "kurgan_b", [700.0 + i as f64 * 80.0, 4400.0], 180.0, Some("21")))
        .collect();
    let mut sc = scenario(10 * 3600, blue, red);
    sc.objectives = vec![ObjectiveDef { name: "OBJ".into(), centre: [800.0, 1000.0], radius_m: 300.0 }];
    sc.sides[1].ai = Some(AiPlan {
        tasks: vec![AiTask {
            platoons: vec!["21".into()],
            mission: Mission::Attack { axis: vec![[800.0, 3000.0]], objective: "OBJ".into() },
            start_s: 0.0,
            release_on_contact: false,
            latest_s: None,
        }],
    });
    (ds, m, sc)
}

#[test]
fn a_recorded_game_replays_to_the_same_state() {
    let (ds, m, sc) = setup();
    let mut sim = Sim::new(&sc, ds.clone(), m.clone()).unwrap();
    for t in 0..(400 * sim::TICK_HZ) {
        match t {
            40 => sim.submit(
                BLUE,
                Command::OccupyPosition {
                    platoon: PlatoonId(1),
                    centre: [800.0, 1250.0],
                    radius_m: 200.0,
                    facing: 0.0,
                    kind: sim::PositionKind::HullDown,
                },
            ),
            2000 => sim.submit(
                BLUE,
                Command::WeaponsControl { to: Addressee::Unit(EntityId(2)), status: WeaponsStatus::Tight },
            ),
            _ => {}
        }
        sim.step();
    }
    let rec = Replay::record(&sim);
    let text = rec.to_ron();
    let back = Replay::from_ron(&text).unwrap();
    assert!(back.commands.iter().all(|c| !c.generated), "AI orders are not part of the replay input");
    let mut again = Sim::new(&sc, ds, m).unwrap();
    back.play(&mut again, |_, _| {});
    assert_eq!(again.tick(), rec.final_tick);
    assert_eq!(again.state_hash(), rec.final_hash, "replay diverged");
}

#[test]
fn enemy_crossing_a_phase_line_releases_the_planned_order() {
    let (ds, m, sc) = setup();
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    // PL across the front at y = 2600 (Blue is south of it, Red attacks from the north).
    let pl = Graphic {
        id: 0,
        label: "PL RED".into(),
        kind: GraphicKind::PhaseLine { points: vec![[0.0, 2600.0], [1600.0, 2600.0]] },
    };
    sim.submit_immediate(BLUE, Command::AddGraphic { graphic: pl });
    sim.submit_immediate(
        BLUE,
        Command::WeaponsControl { to: Addressee::Platoon(PlatoonId(1)), status: WeaponsStatus::Hold },
    );
    run(&mut sim, 1.0);
    let line = sim.perceived(BLUE).graphics[0].id;
    let then = Command::PlatoonMove {
        platoon: PlatoonId(1),
        destination: [800.0, 1300.0],
        via: vec![],
        formation: None,
        technique: Technique::Traveling,
        speed_limit_mps: None,
    };
    sim.submit_immediate(
        BLUE,
        Command::AddTrigger { condition: TriggerCondition::EnemyCrosses { line }, then: Box::new(then) },
    );
    sim.submit_immediate(
        BLUE,
        Command::AddTrigger {
            condition: TriggerCondition::AtTime { time_s: 30.0 },
            then: Box::new(Command::WeaponsControl {
                to: Addressee::Platoon(PlatoonId(1)),
                status: WeaponsStatus::Hold,
            }),
        },
    );
    let mut fired = Vec::new();
    for _ in 0..600 {
        for e in run(&mut sim, 1.0) {
            if let Event::TriggerFired { side: BLUE, id } = e {
                fired.push((id, sim.time_s()));
            }
        }
        if fired.len() == 2 {
            break;
        }
    }
    assert!(fired.iter().any(|&(id, t)| id == 2 && (30.0..32.0).contains(&t)), "timed trigger: {fired:?}");
    let (_, t_pl) = *fired.iter().find(|f| f.0 == 1).expect("PL trigger fired");
    // When it fired, Blue's picture held a contact south of the line.
    assert!(sim.perceived(BLUE).contacts.iter().any(|c| c.pos[1] < 2600.0), "fired at {t_pl:.0} s");
    assert!(sim.perceived(RED).own.iter().any(|u| u.pos[1] < 2700.0));
    let _ = RED;
}

#[test]
fn planning_phase_measures_show_at_once_and_replay_identically() {
    let (ds, m, sc) = setup();
    let mut sim = Sim::new(&sc, ds.clone(), m.clone()).unwrap();
    // Red is played by a human here: its commander AI stays off, and the replay must know.
    sim.set_ai_enabled(RED, false);
    let pl = Graphic {
        id: 0,
        label: "PL AMBER".into(),
        kind: GraphicKind::PhaseLine { points: vec![[0.0, 1800.0], [1600.0, 1800.0]] },
    };
    sim.submit_immediate(BLUE, Command::AddGraphic { graphic: pl });
    sim.submit_immediate(
        BLUE,
        Command::AddTrigger {
            condition: TriggerCondition::AtTime { time_s: 20.0 },
            then: Box::new(Command::PlatoonMove {
                platoon: PlatoonId(1),
                destination: [800.0, 1500.0],
                via: vec![],
                formation: None,
                technique: Technique::Traveling,
                speed_limit_mps: None,
            }),
        },
    );
    sim.settle();
    assert_eq!(sim.tick(), 0, "settle does not advance time");
    assert_eq!(sim.perceived(BLUE).graphics.len(), 1, "graphic visible during planning");
    assert_eq!(sim.perceived(BLUE).triggers.len(), 1, "trigger visible during planning");
    run(&mut sim, 90.0);
    assert!(sim.perceived(BLUE).triggers[0].fired);
    let rec = Replay::record(&sim);
    assert_eq!(rec.player_sides, vec![RED]);
    let back = Replay::from_ron(&rec.to_ron()).unwrap();
    let mut again = Sim::new(&sc, ds, m).unwrap();
    back.play(&mut again, |_, _| {});
    assert_eq!(again.state_hash(), rec.final_hash, "replay diverged");
}
