//! Checkpoint plans (DESIGN.md §8.3): per-leg settings, pass-through checkpoints, waits, and the
//! interaction with other orders.

mod common;

use ai::picture::{ConditionSet, LegSettings, Plan, PlanStep, PlatoonId, SpeedSetting, StepAction, TriggerCondition};
use common::*;
use sim::{Addressee, Command, Event, PlatoonReport, Replay, SideId, Sim};
use sim_data::{Formation, Technique};

const BLUE: SideId = SideId(0);
const P1: PlatoonId = PlatoonId(1);
const KMH: f64 = 1.0 / 3.6;

fn setup() -> (std::sync::Arc<sim_data::DataSet>, std::sync::Arc<terrain::BakedMap>, sim_data::ScenarioDef) {
    let ds = data();
    let m = map(&ds, 400, 1400, "cropland", |_, _| 0.0);
    let blue: Vec<_> = (0..4)
        .map(|i| unit(&format!("1-{}", i + 1), "bulwark_a1", [700.0 + i as f64 * 60.0, 400.0], 0.0, Some("1")))
        .collect();
    let red = vec![unit("211", "kurgan_b", [800.0, 5400.0], 180.0, Some("21"))];
    let sc = scenario(10 * 3600, blue, red);
    (ds, m, sc)
}

fn new_sim() -> Sim {
    let (ds, m, sc) = setup();
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    sim.set_ai_enabled(SideId(1), false);
    sim
}

fn step_at(id: u16, at: [f64; 2]) -> PlanStep {
    PlanStep::new(id, at)
}

fn send(sim: &mut Sim, plan: Plan, start: u16) {
    sim.submit_immediate(BLUE, Command::SetPlan { platoon: P1, plan, start });
}

fn centroid(sim: &Sim) -> [f64; 2] {
    let w = sim.truth();
    let ks: Vec<usize> = (0..w.len()).filter(|&k| w.side[k] == BLUE).collect();
    let n = ks.len() as f64;
    [
        ks.iter().map(|&k| w.transform[k].pos[0]).sum::<f64>() / n,
        ks.iter().map(|&k| w.transform[k].pos[1]).sum::<f64>() / n,
    ]
}

/// Distance of the member nearest to `p`.
fn nearest_member(sim: &Sim, p: [f64; 2]) -> f64 {
    let w = sim.truth();
    (0..w.len()).filter(|&k| w.side[k] == BLUE).map(|k| dist(w.transform[k].pos, p)).fold(f64::INFINITY, f64::min)
}

fn max_speed(sim: &Sim) -> f64 {
    let w = sim.truth();
    (0..w.len())
        .filter(|&k| w.side[k] == BLUE)
        .filter_map(|k| w.mobility[k].as_ref().map(|m| m.speed.abs()))
        .fold(0.0, f64::max)
}

fn reports(ev: &[Event]) -> Vec<PlatoonReport> {
    ev.iter()
        .filter_map(|e| match e {
            Event::PlatoonReport { side, report, .. } if *side == BLUE => Some(report.clone()),
            _ => None,
        })
        .collect()
}

fn view_task(sim: &Sim) -> String {
    sim.perceived(BLUE).platoon(P1).unwrap().task.clone()
}

/// Two legs: MAX in column, then 15 km/h in wedge.
fn two_leg_plan() -> Plan {
    let mut a = step_at(1, [800.0, 1400.0]);
    a.leg = LegSettings {
        speed: Some(SpeedSetting::Max),
        formation: Some(Formation::Column),
        technique: Some(Technique::Traveling),
        ..Default::default()
    };
    let mut b = step_at(2, [800.0, 2200.0]);
    b.leg = LegSettings {
        speed: Some(SpeedSetting::Limit(15.0 * KMH)),
        formation: Some(Formation::Wedge),
        technique: Some(Technique::TravelingOverwatch),
        ..Default::default()
    };
    Plan { steps: vec![a, b] }
}

#[test]
fn legs_apply_their_own_speed_and_formation() {
    let mut sim = new_sim();
    send(&mut sim, two_leg_plan(), 1);
    let mut cp1_passed_at = None;
    let mut done_at = None;
    let mut worst_after = 0.0f64;
    for t in 0..(700 * sim::TICK_HZ) {
        let ev = sim.step();
        let secs = t as f64 / sim::TICK_HZ as f64;
        if cp1_passed_at.is_none() && nearest_member(&sim, [800.0, 1400.0]) < 30.0 {
            cp1_passed_at = Some(secs);
        }
        if let Some(c) = cp1_passed_at
            && secs > c + 20.0
        {
            worst_after = worst_after.max(max_speed(&sim));
            let v = sim.perceived(BLUE).platoon(P1).unwrap().clone();
            assert_eq!(v.formation, Formation::Wedge, "wedge within 20 s of CP 1");
        }
        if reports(&ev).contains(&PlatoonReport::PlanComplete) {
            done_at = Some(secs);
            break;
        }
    }
    let done = done_at.expect("plan completes");
    assert!(worst_after <= 15.0 * KMH * 1.15, "no member exceeds the limit after CP 1: {worst_after:.2} m/s");
    assert!(done < 450.0, "arrives in time: {done}");
    assert!(nearest_member(&sim, [800.0, 2200.0]) < 30.0);
    assert!(sim.perceived(BLUE).platoon(P1).unwrap().plan.is_none(), "finished plan is gone");
}

#[test]
fn a_pass_through_checkpoint_does_not_stop_the_platoon() {
    let mut sim = new_sim();
    let mut plan = two_leg_plan();
    plan.steps[1].leg = LegSettings::default();
    send(&mut sim, plan, 1);
    let mut slowest_near_cp = f64::INFINITY;
    for _ in 0..(300 * sim::TICK_HZ) {
        sim.step();
        if nearest_member(&sim, [800.0, 1400.0]) < 60.0 {
            slowest_near_cp = slowest_near_cp.min(max_speed(&sim));
        }
    }
    assert!(slowest_near_cp.is_finite(), "platoon passed CP 1");
    assert!(slowest_near_cp > 2.0, "keeps moving through CP 1: {slowest_near_cp:.2}");
}

#[test]
fn an_elapsed_wait_holds_the_platoon_then_releases_it() {
    let mut sim = new_sim();
    let mut plan = two_leg_plan();
    plan.steps[0].wait = Some(ConditionSet { all: true, conditions: vec![TriggerCondition::Elapsed { s: 120.0 }] });
    plan.steps[1].leg = LegSettings::default();
    send(&mut sim, plan, 1);
    let mut stopped_since = None;
    let mut still_for = 0.0;
    let mut moved_after = None;
    let mut saw_wait_report = false;
    for t in 0..(500 * sim::TICK_HZ) {
        let ev = sim.step();
        saw_wait_report |= reports(&ev).contains(&PlatoonReport::AtCheckpoint { n: 1, waiting: true });
        let secs = t as f64 / sim::TICK_HZ as f64;
        let at_cp = nearest_member(&sim, [800.0, 1400.0]) < 30.0;
        if at_cp && max_speed(&sim) < 0.2 && view_task(&sim).starts_with("WAITING") {
            stopped_since.get_or_insert(secs);
        } else if let Some(s) = stopped_since
            && max_speed(&sim) > 0.5
            && moved_after.is_none()
        {
            still_for = secs - s;
            moved_after = Some(secs);
        }
    }
    assert!(saw_wait_report);
    assert!(moved_after.is_some(), "platoon departs after the wait");
    assert!((115.0..=135.0).contains(&still_for), "stationary about 120 s, was {still_for:.0}");
}

#[test]
fn other_orders_and_drills_treat_the_plan_correctly() {
    let mut sim = new_sim();
    send(&mut sim, two_leg_plan(), 1);
    run(&mut sim, 30.0);
    assert!(view_task(&sim).starts_with("PLAN CP"), "{}", view_task(&sim));
    // HALT pauses, RESUME continues the same step.
    sim.submit_immediate(BLUE, Command::Halt { to: Addressee::Platoon(P1) });
    run(&mut sim, 15.0);
    let held = sim.perceived(BLUE).platoon(P1).unwrap().clone();
    assert!(held.plan.is_some() && held.held, "halt keeps the plan");
    let id = held.plan.as_ref().unwrap().cursor_id;
    let y = centroid(&sim)[1];
    run(&mut sim, 20.0);
    assert!((centroid(&sim)[1] - y).abs() < 5.0, "stays halted");
    sim.submit_immediate(BLUE, Command::Resume { to: Addressee::Platoon(P1) });
    run(&mut sim, 20.0);
    let v = sim.perceived(BLUE).platoon(P1).unwrap().clone();
    assert!(centroid(&sim)[1] > y + 20.0, "moving again");
    assert_eq!(v.plan.as_ref().unwrap().cursor_id, id, "same step after resume");
    // Posture orders keep the plan.
    sim.submit_immediate(BLUE, Command::SetFormation { platoon: P1, formation: Formation::Line });
    run(&mut sim, 3.0);
    assert!(sim.perceived(BLUE).platoon(P1).unwrap().plan.is_some());
    // A player move discards it.
    sim.submit_immediate(
        BLUE,
        Command::PlatoonMove {
            platoon: P1,
            destination: [1000.0, 1000.0],
            via: vec![],
            formation: None,
            technique: Technique::Traveling,
            speed_limit_mps: None,
        },
    );
    run(&mut sim, 3.0);
    assert!(sim.perceived(BLUE).platoon(P1).unwrap().plan.is_none(), "player move discards the plan");
}

#[test]
fn a_contact_drill_mid_leg_resumes_the_same_step() {
    let ds = data();
    let m = map(&ds, 400, 1400, "cropland", |_, _| 0.0);
    let blue: Vec<_> = (0..4)
        .map(|i| unit(&format!("1-{}", i + 1), "bulwark_a1", [700.0 + i as f64 * 60.0, 400.0], 0.0, Some("1")))
        .collect();
    // A lone enemy ahead, close to the route, holding fire is not needed: it just sits there.
    let red = vec![unit("211", "kurgan_b", [1300.0, 1500.0], 270.0, Some("21"))];
    let sc = scenario(10 * 3600, blue, red);
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    sim.set_ai_enabled(SideId(1), false);
    send(&mut sim, two_leg_plan(), 1);
    let mut contact_cursor = None;
    let mut resumed_cursor = None;
    for _ in 0..(500 * sim::TICK_HZ) {
        let ev = sim.step();
        let rs = reports(&ev);
        let cursor = sim.perceived(BLUE).platoon(P1).unwrap().plan.as_ref().map(|p| p.cursor_id);
        if contact_cursor.is_none() && rs.iter().any(|r| matches!(r, PlatoonReport::Contact { .. })) {
            contact_cursor = Some(cursor.expect("the drill does not drop the plan"));
        }
        if contact_cursor.is_some() && resumed_cursor.is_none() && rs.contains(&PlatoonReport::ContinuingMission) {
            resumed_cursor = cursor;
        }
    }
    let c = contact_cursor.expect("the scenario provokes a contact drill");
    assert_eq!(resumed_cursor, Some(c), "the drill resumes the same step");
}

#[test]
fn editing_a_running_plan_keeps_progress() {
    let mut sim = new_sim();
    let plan = two_leg_plan();
    send(&mut sim, plan.clone(), 1);
    run(&mut sim, 150.0);
    let v = sim.perceived(BLUE).platoon(P1).unwrap().plan.clone().unwrap();
    assert_eq!(v.cursor_id, 2, "CP 1 passed");
    assert_eq!(v.passed, vec![1]);
    let mut edited = plan;
    edited.steps[1].at = [900.0, 2300.0];
    send(&mut sim, edited, v.cursor_id);
    run(&mut sim, 3.0);
    let v = sim.perceived(BLUE).platoon(P1).unwrap().plan.clone().unwrap();
    assert_eq!(v.cursor_id, 2);
    assert_eq!(v.passed, vec![1], "progress kept");
    assert_eq!(v.plan.steps[1].at, [900.0, 2300.0]);
    // An empty plan clears it.
    send(&mut sim, Plan::default(), 0);
    run(&mut sim, 3.0);
    assert!(sim.perceived(BLUE).platoon(P1).unwrap().plan.is_none());
}

#[test]
fn a_replay_with_a_plan_reproduces_the_state() {
    let (ds, m, sc) = setup();
    let mut sim = Sim::new(&sc, ds.clone(), m.clone()).unwrap();
    sim.set_ai_enabled(SideId(1), false);
    let mut plan = two_leg_plan();
    plan.steps[0].actions = vec![StepAction::Face { bearing: 0.5 }, StepAction::Report];
    plan.steps[0].wait = Some(ConditionSet { all: false, conditions: vec![TriggerCondition::Elapsed { s: 30.0 }] });
    send(&mut sim, plan, 1);
    run(&mut sim, 200.0);
    let rec = Replay::record(&sim);
    let text = rec.to_ron();
    let back = Replay::from_ron(&text).expect("RON round trip of SetPlan");
    assert!(back.commands.iter().any(|c| matches!(c.cmd, Command::SetPlan { .. })));
    let mut again = Sim::new(&sc, ds, m).unwrap();
    again.set_ai_enabled(SideId(1), false);
    back.play(&mut again, |_, _| {});
    assert_eq!(again.state_hash(), rec.final_hash, "replay diverged");
}
