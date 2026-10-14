//! M8a acceptance tests: order queue, reverse driving, weather effects on the going.

mod common;

use ai::picture::{PlatoonId, PositionKind};
use common::*;
use sim::{Addressee, Command, SideId, Sim};
use sim_data::{Formation, Technique, WeatherDef};

const BLUE: SideId = SideId(0);
const P1: PlatoonId = PlatoonId(1);

fn platoon_sim(weather: &str, slope: f64) -> Sim {
    let ds = data();
    let m = map(&ds, 400, 1000, "cropland", move |_, y| y * slope);
    let blue: Vec<_> = (0..4)
        .map(|i| unit(&format!("1-{}", i + 1), "bulwark_a1", [700.0 + i as f64 * 60.0, 300.0], 0.0, Some("1")))
        .collect();
    let red = vec![unit("211", "kurgan_b", [800.0, 3800.0], 180.0, Some("21"))];
    let mut sc = scenario(10 * 3600, blue, red);
    sc.weather = WeatherDef { preset: weather.into(), ..WeatherDef::default() };
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    sim.set_ai_enabled(SideId(1), false);
    sim
}

fn lead_y(sim: &Sim) -> f64 {
    sim.truth().transform[0].pos[1]
}

fn pmove(to: [f64; 2]) -> Command {
    Command::PlatoonMove {
        platoon: P1,
        destination: to,
        via: vec![],
        formation: Some(Formation::Column),
        technique: Technique::Traveling,
        speed_limit_mps: None,
    }
}

fn bp(at: [f64; 2]) -> Command {
    Command::OccupyPosition { platoon: P1, centre: at, radius_m: 80.0, facing: 0.0, kind: PositionKind::HullDown }
}

fn task(sim: &Sim) -> String {
    sim.perceived(BLUE).platoon(P1).unwrap().task.clone()
}

#[test]
fn rain_slows_a_climb_against_the_same_hill_in_clear_weather() {
    // Wong, Theory of Ground Vehicles ch. 2: wet soil raises motion resistance, so a
    // power-limited tank on a grade is slower.
    let reach = |w: &str| {
        let mut sim = platoon_sim(w, 0.12);
        sim.submit_immediate(
            BLUE,
            Command::Move { unit: sim.truth().id[0], destination: [700.0, 3000.0], via: vec![], speed_limit_mps: None },
        );
        run(&mut sim, 60.0);
        lead_y(&sim)
    };
    let (dry, wet) = (reach("clear"), reach("rain"));
    assert!(wet < dry - 5.0, "rain {wet:.0} m vs clear {dry:.0} m");
}

#[test]
fn a_tank_backs_up_straight_at_reverse_speed() {
    let mut sim = platoon_sim("clear", 0.0);
    let id = sim.truth().id[0];
    let (start, heading) = (sim.truth().transform[0].pos, sim.truth().transform[0].heading);
    sim.submit_immediate(BLUE, Command::Reverse { to: Addressee::Unit(id), distance_m: 80.0 });
    let mut vmax = 0.0f64;
    for _ in 0..(60 * sim::TICK_HZ) {
        sim.step();
        vmax = vmax.max(sim.truth().mobility[0].as_ref().unwrap().speed);
    }
    let t = &sim.truth().transform[0];
    let back = start[1] - t.pos[1];
    assert!((back - 80.0).abs() < 4.0, "backed {back:.1} m");
    assert!((t.pos[0] - start[0]).abs() < 1.0, "drifted sideways");
    assert!((t.heading - heading).abs() < 0.02, "hull turned");
    assert!(vmax <= 4.95, "reverse speed {vmax:.1} m/s is over the reverse limit");
    assert!(vmax > 2.0, "never reached a reverse speed ({vmax:.1} m/s)");
}

#[test]
fn a_move_after_reversing_drives_forward_again() {
    let mut sim = platoon_sim("clear", 0.0);
    let id = sim.truth().id[0];
    sim.submit_immediate(BLUE, Command::Reverse { to: Addressee::Unit(id), distance_m: 40.0 });
    run(&mut sim, 20.0);
    let y0 = lead_y(&sim);
    sim.submit_immediate(
        BLUE,
        Command::Move { unit: id, destination: [700.0, 500.0], via: vec![], speed_limit_mps: None },
    );
    run(&mut sim, 40.0);
    assert!(lead_y(&sim) > y0 + 100.0, "{} vs {}", lead_y(&sim), y0);
}

#[test]
fn queued_orders_run_in_turn_after_the_platoon_finishes() {
    let mut sim = platoon_sim("clear", 0.0);
    sim.submit_immediate(BLUE, pmove([800.0, 700.0]));
    sim.submit_immediate(BLUE, Command::QueueOrder { platoon: P1, then: Box::new(bp([800.0, 900.0])) });
    run(&mut sim, 2.0);
    assert_eq!(sim.perceived(BLUE).platoon(P1).unwrap().queued, 1);
    assert_eq!(task(&sim), "MOVING");
    // Mid-move the BP order has not started.
    run(&mut sim, 20.0);
    assert_eq!(task(&sim), "MOVING");
    run(&mut sim, 200.0);
    let v = sim.perceived(BLUE);
    let p = v.platoon(P1).unwrap();
    assert_eq!(p.queued, 0);
    assert!(p.task.contains("BATTLE POSITION"), "{}", p.task);
}

#[test]
fn a_new_direct_order_clears_the_queue_and_halt_keeps_it() {
    let mut sim = platoon_sim("clear", 0.0);
    sim.submit_immediate(BLUE, pmove([800.0, 900.0]));
    sim.submit_immediate(BLUE, Command::QueueOrder { platoon: P1, then: Box::new(bp([800.0, 950.0])) });
    run(&mut sim, 5.0);
    sim.submit_immediate(BLUE, Command::Halt { to: Addressee::Platoon(P1) });
    run(&mut sim, 30.0);
    let v = sim.perceived(BLUE);
    assert_eq!(v.platoon(P1).unwrap().queued, 1, "halt must keep the queue");
    sim.submit_immediate(BLUE, pmove([800.0, 600.0]));
    run(&mut sim, 2.0);
    assert_eq!(sim.perceived(BLUE).platoon(P1).unwrap().queued, 0);
}
