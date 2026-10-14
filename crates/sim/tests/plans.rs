//! Checkpoint plans (DESIGN.md §8.3): per-leg settings, pass-through checkpoints, waits, and the
//! interaction with other orders.

mod common;

use ai::picture::{
    Branch, ConditionSet, ContactReaction, FireKind, Graphic, GraphicKind, LegSettings, Plan, PlanStep, PlatoonId,
    PositionKind, SpeedSetting, StepAction, TriggerCondition,
};
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
fn a_platoon_in_line_that_lost_its_leader_still_arrives() {
    // The leader's slot is on the checkpoint; with it knocked out the survivors stand a slot or
    // two away from it, and that is still arrival (not a failed route that ends the plan).
    let mut sim = new_sim();
    sim.truth_mut().condition[0] = sim::view::Condition::Destroyed;
    let mut a = step_at(1, [800.0, 1400.0]);
    a.leg.formation = Some(Formation::Line);
    a.wait = Some(ConditionSet { all: false, conditions: vec![TriggerCondition::Elapsed { s: 5.0 }] });
    send(&mut sim, Plan { steps: vec![a, step_at(2, [800.0, 1800.0])] }, 1);
    let mut rs = Vec::new();
    for _ in 0..(400 * sim::TICK_HZ) {
        rs.extend(reports(&sim.step()));
        if rs.contains(&PlatoonReport::PlanComplete) {
            break;
        }
    }
    assert!(rs.contains(&PlatoonReport::AtCheckpoint { n: 1, waiting: true }), "arrived at CP 1: {rs:?}");
    assert!(rs.contains(&PlatoonReport::PlanComplete), "plan runs on: {rs:?}");
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

fn elapsed(s: f64) -> Option<ConditionSet> {
    Some(ConditionSet { all: true, conditions: vec![TriggerCondition::Elapsed { s }] })
}

fn plan_view(sim: &Sim) -> ai::picture::PlanView {
    sim.perceived(BLUE).platoon(P1).unwrap().plan.clone().expect("plan running")
}

/// CONTROL-UX §7: a posture order during a leg is written into the leg in progress, up to the
/// next stop, and the leg after that stop keeps its planned setting.
#[test]
fn a_speed_order_holds_through_pass_through_checkpoints_up_to_the_next_stop() {
    let mut sim = new_sim();
    let slow = LegSettings { speed: Some(SpeedSetting::Limit(15.0 * KMH)), ..Default::default() };
    let mut s1 = step_at(1, [800.0, 900.0]);
    s1.wait = elapsed(1.0);
    let mut s2 = step_at(2, [800.0, 1300.0]);
    s2.leg = slow.clone();
    let mut s3 = step_at(3, [800.0, 1800.0]);
    s3.leg = slow;
    s3.wait = elapsed(5.0);
    let mut s4 = step_at(4, [800.0, 2300.0]);
    s4.leg = LegSettings { speed: Some(SpeedSetting::Limit(10.0 * KMH)), ..Default::default() };
    send(&mut sim, Plan { steps: vec![s1, s2, s3, s4] }, 1);

    // Leave CP 1 and be on the leg towards CP 2.
    let mut t = 0;
    while !(view_task(&sim).starts_with("PLAN CP 2") && centroid(&sim)[1] > 1000.0) {
        sim.step();
        t += 1;
        assert!(t < 400 * sim::TICK_HZ, "reached the leg: {}", view_task(&sim));
    }
    sim.submit_immediate(BLUE, Command::SetSpeed { to: Addressee::Platoon(P1), speed_limit_mps: None });
    sim.step();
    let v = plan_view(&sim);
    let speed = |i: usize| v.plan.steps[i].leg.speed;
    assert_eq!(speed(0), None, "the leg already driven is not touched");
    assert_eq!(speed(1), Some(SpeedSetting::Max), "pass-through CP 2 rewritten");
    assert_eq!(speed(2), Some(SpeedSetting::Max), "the leg ends at the stop CP 3");
    assert_eq!(speed(3), Some(SpeedSetting::Limit(10.0 * KMH)), "later legs are never touched");

    // MAX holds through CP 2 ...
    let mut fastest_after_cp2 = 0.0f64;
    for _ in 0..(120 * sim::TICK_HZ) {
        sim.step();
        if centroid(&sim)[1] > 1400.0 && centroid(&sim)[1] < 1700.0 {
            fastest_after_cp2 = fastest_after_cp2.max(max_speed(&sim));
        }
        if view_task(&sim).starts_with("WAITING") {
            break;
        }
    }
    assert!(fastest_after_cp2 > 30.0 * KMH, "still at MAX after CP 2: {fastest_after_cp2:.1} m/s");
    // ... and leg 4's planned limit applies after the stop.
    let mut worst = 0.0f64;
    let mut left = false;
    for _ in 0..(200 * sim::TICK_HZ) {
        sim.step();
        if centroid(&sim)[1] > 1900.0 {
            left = true;
        }
        if centroid(&sim)[1] > 2000.0 {
            worst = worst.max(max_speed(&sim));
        }
    }
    assert!(left, "departed CP 3");
    assert!(worst <= 10.0 * KMH * 1.15, "leg 4 planned limit applies: {worst:.2} m/s");
}

#[test]
fn a_posture_order_at_a_waiting_checkpoint_applies_now_and_the_next_leg_keeps_its_plan() {
    let mut sim = new_sim();
    let mut a = step_at(1, [800.0, 1000.0]);
    a.wait = elapsed(60.0);
    a.leg.formation = Some(Formation::Column);
    let mut b = step_at(2, [800.0, 1600.0]);
    b.leg.formation = Some(Formation::Wedge);
    send(&mut sim, Plan { steps: vec![a, b] }, 1);
    let mut t = 0;
    while !view_task(&sim).starts_with("WAITING") {
        sim.step();
        t += 1;
        assert!(t < 400 * sim::TICK_HZ, "waiting: {}", view_task(&sim));
    }
    sim.submit_immediate(BLUE, Command::SetFormation { platoon: P1, formation: Formation::Line });
    sim.step();
    assert_eq!(sim.perceived(BLUE).platoon(P1).unwrap().formation, Formation::Line, "applies now");
    let v = plan_view(&sim);
    assert_eq!(v.plan.steps[0].leg.formation, Some(Formation::Column), "no leg in progress: not rewritten");
    assert_eq!(v.plan.steps[1].leg.formation, Some(Formation::Wedge));
    for _ in 0..(90 * sim::TICK_HZ) {
        sim.step();
    }
    assert_eq!(sim.perceived(BLUE).platoon(P1).unwrap().formation, Formation::Wedge, "next leg's plan on release");
}

#[test]
fn set_technique_changes_the_move_in_progress_and_replays() {
    let (ds, m, sc) = setup();
    let mut sim = Sim::new(&sc, ds.clone(), m.clone()).unwrap();
    sim.set_ai_enabled(SideId(1), false);
    let mut a = step_at(1, [800.0, 1500.0]);
    a.leg.technique = Some(Technique::Traveling);
    send(&mut sim, Plan { steps: vec![a] }, 1);
    run(&mut sim, 20.0);
    sim.submit_immediate(BLUE, Command::SetTechnique { platoon: P1, technique: Technique::BoundingOverwatch });
    run(&mut sim, 2.0);
    assert_eq!(sim.perceived(BLUE).platoon(P1).unwrap().technique, Technique::BoundingOverwatch);
    assert_eq!(plan_view(&sim).plan.steps[0].leg.technique, Some(Technique::BoundingOverwatch));
    let rec = Replay::record(&sim);
    let back = Replay::from_ron(&rec.to_ron()).expect("RON round trip of SetTechnique");
    assert!(back.commands.iter().any(|c| matches!(c.cmd, Command::SetTechnique { .. })));
    let mut again = Sim::new(&sc, ds, m).unwrap();
    again.set_ai_enabled(SideId(1), false);
    back.play(&mut again, |_, _| {});
    assert_eq!(again.state_hash(), rec.final_hash, "replay diverged");
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

// ---- P2: conditions, actions, contact reaction, code words ----

const P2: PlatoonId = PlatoonId(2);

fn waits(c: TriggerCondition) -> Option<ConditionSet> {
    Some(ConditionSet { all: true, conditions: vec![c] })
}

fn send_to(sim: &mut Sim, platoon: PlatoonId, plan: Plan) {
    let start = plan.steps[0].id;
    sim.submit_immediate(BLUE, Command::SetPlan { platoon, plan, start });
}

fn task_of(sim: &Sim, p: PlatoonId) -> String {
    sim.perceived(BLUE).platoon(p).unwrap().task.clone()
}

/// Two 2-tank platoons and a battery.
fn two_platoon_sim() -> Sim {
    let ds = data();
    let m = map(&ds, 400, 1400, "cropland", |_, _| 0.0);
    let mut blue: Vec<_> = (0..2)
        .map(|i| unit(&format!("1-{}", i + 1), "bulwark_a1", [500.0 + i as f64 * 60.0, 400.0], 0.0, Some("1")))
        .collect();
    blue.extend(
        (0..2).map(|i| unit(&format!("2-{}", i + 1), "bulwark_a1", [1100.0 + i as f64 * 60.0, 400.0], 0.0, Some("2"))),
    );
    let red = vec![unit("211", "kurgan_b", [800.0, 5400.0], 180.0, Some("21"))];
    let mut sc = scenario(10 * 3600, blue, red);
    sc.sides[0].fire_support = vec![sim_data::BatteryAssign {
        callsign: "ARTY 1".into(),
        battery: "sph155_btry".into(),
        position: [200.0, 100.0],
        he_rounds: None,
        smoke_rounds: None,
    }];
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    sim.set_ai_enabled(SideId(1), false);
    sim
}

fn platoon_speed(sim: &Sim, p: PlatoonId) -> f64 {
    let w = sim.truth();
    (0..w.len())
        .filter(|&k| w.platoon[k] == Some(p))
        .filter_map(|k| w.mobility[k].as_ref().map(|m| m.speed.abs()))
        .fold(0.0, f64::max)
}

#[test]
fn a_code_word_releases_every_waiting_platoon_in_the_same_tick() {
    let mut sim = two_platoon_sim();
    let cp = |x: f64| {
        let mut a = step_at(1, [x, 900.0]);
        a.wait = waits(TriggerCondition::CodeWord { word: 0 });
        a.on_release = vec![
            StepAction::SmokeScreen,
            StepAction::CallForFire { battery: 0, at: [800.0, 3000.0], kind: FireKind::Smoke, volleys: 1 },
        ];
        Plan { steps: vec![a, step_at(2, [x, 1600.0])] }
    };
    send_to(&mut sim, P1, cp(550.0));
    send_to(&mut sim, P2, cp(1150.0));
    run(&mut sim, 300.0);
    for p in [P1, P2] {
        assert!(task_of(&sim, p).starts_with("WAITING"), "{p:?} holds: {}", task_of(&sim, p));
        assert!(platoon_speed(&sim, p) < 0.2, "{p:?} stationary");
    }
    let word_tick = sim.tick();
    sim.submit_immediate(BLUE, Command::CodeWord { word: 0 });
    let mut left: [Option<u64>; 2] = [None, None];
    let mut smoke_tick = None;
    let mut fire_tick = None;
    for _ in 0..(5 * sim::TICK_HZ) {
        let t = sim.tick();
        let ev = sim.step();
        if smoke_tick.is_none() && ev.iter().any(|e| matches!(e, Event::SmokeLaunched { .. })) {
            smoke_tick = Some(t);
        }
        if fire_tick.is_none()
            && ev.iter().any(|e| matches!(e, Event::FireSupport { report: sim::FireReport::Accepted { .. }, .. }))
        {
            fire_tick = Some(t);
        }
        for (i, p) in [P1, P2].into_iter().enumerate() {
            if left[i].is_none() && !task_of(&sim, p).starts_with("WAITING") {
                left[i] = Some(t);
            }
        }
    }
    let (l1, l2) = (left[0].expect("P1 released"), left[1].expect("P2 released"));
    assert_eq!(l1, l2, "both platoons depart in the same tick");
    assert!((l1 - word_tick) as f64 / sim::TICK_HZ as f64 <= 2.0, "within 2 s of the word");
    assert!(smoke_tick.expect("smoke launched") <= l1, "on_release runs before the departure");
    assert!(fire_tick.expect("fire mission called") <= l1);
    run(&mut sim, 15.0);
    assert!(platoon_speed(&sim, P1) > 1.0 && platoon_speed(&sim, P2) > 1.0, "both under way");
}

#[test]
fn the_wrong_word_or_one_sent_before_arrival_does_not_release() {
    let mut sim = new_sim();
    let mut a = step_at(1, [800.0, 1400.0]);
    a.actions = vec![StepAction::OccupyBp { radius_m: 60.0, facing: 0.0, kind: PositionKind::HullDown }];
    a.wait = waits(TriggerCondition::CodeWord { word: 1 });
    send(&mut sim, Plan { steps: vec![a, step_at(2, [800.0, 2000.0])] }, 1);
    sim.submit_immediate(BLUE, Command::CodeWord { word: 1 });
    let mut set_in_bp = false;
    let mut ev = Vec::new();
    for _ in 0..(300 * sim::TICK_HZ) {
        ev.extend(sim.step());
    }
    for r in reports(&ev) {
        set_in_bp |= matches!(r, PlatoonReport::SetInPosition { .. });
    }
    assert!(set_in_bp, "the platoon reports set in its BP");
    assert!(view_task(&sim).starts_with("WAITING"), "an earlier transmission does not count: {}", view_task(&sim));
    sim.submit_immediate(BLUE, Command::CodeWord { word: 0 });
    run(&mut sim, 20.0);
    assert!(view_task(&sim).starts_with("WAITING"), "another word does not release");
    sim.submit_immediate(BLUE, Command::CodeWord { word: 1 });
    let ev = run(&mut sim, 400.0);
    assert!(reports(&ev).contains(&PlatoonReport::PlanComplete), "released, continues to CP 2");
    assert!(nearest_member(&sim, [800.0, 2000.0]) < 30.0);
}

#[test]
fn continue_keeps_moving_under_contact_where_the_drill_halts() {
    let contacts = |reaction: Option<ContactReaction>| {
        let ds = data();
        let m = map(&ds, 400, 1400, "cropland", |_, _| 0.0);
        let blue: Vec<_> = (0..4)
            .map(|i| unit(&format!("1-{}", i + 1), "bulwark_a1", [700.0 + i as f64 * 60.0, 400.0], 0.0, Some("1")))
            .collect();
        let red = vec![unit("211", "kurgan_b", [1300.0, 1500.0], 270.0, Some("21"))];
        let mut sim = Sim::new(&scenario(10 * 3600, blue, red), ds, m).unwrap();
        sim.set_ai_enabled(SideId(1), false);
        if let Some(r) = reaction {
            sim.submit_immediate(BLUE, Command::SetContactReaction { platoon: P1, reaction: r });
        }
        send(&mut sim, two_leg_plan(), 1);
        let ev = run(&mut sim, 250.0);
        reports(&ev).iter().filter(|r| matches!(r, PlatoonReport::Contact { .. })).count()
    };
    assert!(contacts(None) > 0, "the default drill reacts");
    assert_eq!(contacts(Some(ContactReaction::Continue)), 0, "Continue never enters the drill");
}

#[test]
fn a_contact_branch_diverts_the_platoon_and_runs_the_actions_there() {
    let ds = data();
    let m = map(&ds, 400, 1400, "cropland", |_, _| 0.0);
    let blue: Vec<_> = (0..4)
        .map(|i| unit(&format!("1-{}", i + 1), "bulwark_a1", [700.0 + i as f64 * 60.0, 400.0], 0.0, Some("1")))
        .collect();
    let red = vec![unit("211", "kurgan_b", [1300.0, 2600.0], 270.0, Some("21"))];
    let mut sim = Sim::new(&scenario(10 * 3600, blue, red), ds, m).unwrap();
    sim.set_ai_enabled(SideId(1), false);
    let mut a = step_at(1, [800.0, 1400.0]);
    a.leg.on_contact = Some(ContactReaction::Continue);
    a.branch = Some(Branch {
        when: ConditionSet {
            all: false,
            conditions: vec![TriggerCondition::ContactWithin { platoon: P1, range_m: 4000.0 }],
        },
        goto: 3,
    });
    let mut c = step_at(3, [300.0, 1100.0]);
    c.actions = vec![StepAction::SmokeScreen];
    send(&mut sim, Plan { steps: vec![a, step_at(2, [800.0, 2200.0]), c] }, 1);
    let mut ev = Vec::new();
    for _ in 0..(400 * sim::TICK_HZ) {
        ev.extend(sim.step());
    }
    let rs = reports(&ev);
    assert!(rs.contains(&PlatoonReport::PlanBranch { to: 3 }), "branch taken: {rs:?}");
    assert!(ev.iter().any(|e| matches!(e, Event::SmokeLaunched { .. })), "smoke at the branch CP");
    assert!(rs.contains(&PlatoonReport::PlanComplete), "branch CP is the end of the plan");
    assert!(nearest_member(&sim, [300.0, 1100.0]) < 60.0);
}

#[test]
fn a_platoon_can_wait_for_another_to_reach_its_checkpoint() {
    let mut sim = two_platoon_sim();
    send_to(&mut sim, P1, Plan { steps: vec![step_at(1, [550.0, 1400.0])] });
    let mut a = step_at(1, [1150.0, 500.0]);
    a.wait = waits(TriggerCondition::PlatoonAt { platoon: P1, step: 1 });
    send_to(&mut sim, P2, Plan { steps: vec![a, step_at(2, [1150.0, 1400.0])] });
    let near = |sim: &Sim, p: PlatoonId, at: [f64; 2]| {
        let w = sim.truth();
        (0..w.len())
            .filter(|&k| w.platoon[k] == Some(p))
            .map(|k| dist(w.transform[k].pos, at))
            .fold(f64::INFINITY, f64::min)
    };
    let mut p2_left = None;
    let mut p2_waited = false;
    let mut p1_there = None;
    for t in 0..(300 * sim::TICK_HZ) {
        sim.step();
        let secs = t as f64 / sim::TICK_HZ as f64;
        if p1_there.is_none() && near(&sim, P1, [550.0, 1400.0]) < 60.0 {
            p1_there = Some(secs);
        }
        p2_waited |= task_of(&sim, P2).starts_with("WAITING");
        if p2_waited && p2_left.is_none() && !task_of(&sim, P2).starts_with("WAITING") {
            p2_left = Some(secs);
        }
    }
    let (arrived, left) = (p1_there.expect("P1 arrives"), p2_left.expect("P2 is released"));
    assert!(left >= arrived - 1.0 && left <= arrived + 15.0, "P2 leaves when P1 arrives: {arrived:.0} / {left:.0}");
}

#[test]
fn conditions_read_the_picture_not_the_truth() {
    let run_case = |enemy_at: [f64; 2]| {
        let ds = data();
        // A ridge across the map hides everything behind it.
        let m = map(&ds, 400, 1400, "cropland", |_, y| if (2000.0..2200.0).contains(&y) { 60.0 } else { 0.0 });
        let blue: Vec<_> = (0..4)
            .map(|i| unit(&format!("1-{}", i + 1), "bulwark_a1", [700.0 + i as f64 * 60.0, 400.0], 0.0, Some("1")))
            .collect();
        let red = vec![unit("211", "kurgan_b", enemy_at, 180.0, Some("21"))];
        let mut sim = Sim::new(&scenario(10 * 3600, blue, red), ds, m).unwrap();
        sim.set_ai_enabled(SideId(1), false);
        sim.submit_immediate(
            BLUE,
            Command::AddGraphic {
                graphic: Graphic {
                    id: 0,
                    label: "OBJ A".into(),
                    kind: GraphicKind::Objective { centre: enemy_at, radius: 250.0 },
                },
            },
        );
        let mut a = step_at(1, [800.0, 450.0]);
        a.leg.on_contact = Some(ContactReaction::Continue);
        a.wait = waits(TriggerCondition::EnemyInArea { graphic: 1 });
        send(&mut sim, Plan { steps: vec![a, step_at(2, [800.0, 600.0])] }, 1);
        // The enemy drives about (movement gives it away); nobody fires.
        let red = (0..sim.truth().len()).find(|&k| sim.truth().side[k] == SideId(1)).unwrap();
        let unit = sim.truth().id[red];
        for (side, to) in [(BLUE, Addressee::Platoon(P1)), (SideId(1), Addressee::Unit(unit))] {
            sim.submit_immediate(side, Command::WeaponsControl { to, status: sim::view::WeaponsStatus::Hold });
        }
        let to = [enemy_at[0], enemy_at[1] + if enemy_at[1] < 2000.0 { 300.0 } else { -300.0 }];
        sim.submit_immediate(
            SideId(1),
            Command::Move { unit, destination: to, via: vec![], speed_limit_mps: Some(3.0) },
        );
        let mut known = 0;
        for _ in 0..20 {
            run(&mut sim, 10.0);
            known = known.max(sim.perceived(BLUE).live_contacts().count());
        }
        (!view_task(&sim).starts_with("WAITING"), known)
    };
    let (released, known) = run_case([800.0, 3000.0]);
    assert_eq!((released, known), (false, 0), "an enemy nobody has seen never fires the condition");
    let (released, known) = run_case([800.0, 1000.0]);
    assert!(known > 0 && released, "once reported, the same condition holds: known {known}, released {released}");
}
