//! M7d acceptance tests: called fire support, HE effects, suppression, react-to-indirect-fire.

mod common;

use std::sync::Arc;

use common::*;
use sim::{Command, DenyReason, Event, FireKind, FireReport, PlatoonReport, SideId, Sim};
use sim_data::{BatteryAssign, DataSet, ScenarioDef};
use terrain::BakedMap;

const BLUE: SideId = SideId(0);
const NOON: u32 = 12 * 3600;
const BATTERY: [f64; 2] = [500.0, 500.0];

fn world() -> (Arc<DataSet>, Arc<BakedMap>) {
    let ds = data();
    let m = map(&ds, 1500, 1500, "cropland", |_, _| 0.0);
    (ds, m)
}

fn with_battery(mut sc: ScenarioDef, he: Option<u32>, smoke: Option<u32>) -> ScenarioDef {
    sc.sides[0].fire_support = vec![BatteryAssign {
        callsign: "ARTY 1".into(),
        battery: "sph155_btry".into(),
        position: BATTERY,
        he_rounds: he,
        smoke_rounds: smoke,
    }];
    sc
}

fn call(sim: &mut Sim, at: [f64; 2], kind: FireKind, volleys: u32) {
    sim.submit_immediate(BLUE, Command::CallForFire { battery: 0, at, kind, volleys });
}

fn fire_reports(ev: &[(u64, Event)]) -> Vec<(u64, FireReport)> {
    ev.iter()
        .filter_map(|(t, e)| match e {
            Event::FireSupport { report, .. } => Some((*t, *report)),
            _ => None,
        })
        .collect()
}

fn run_events(sim: &mut Sim, seconds: f64) -> Vec<(u64, Event)> {
    let mut out = Vec::new();
    for _ in 0..(seconds * sim::TICK_HZ as f64) as usize {
        let t = sim.tick();
        out.extend(sim.step().into_iter().map(|e| (t, e)));
    }
    out
}

fn secs(t: u64) -> f64 {
    t as f64 * sim::DT
}

#[test]
fn call_for_fire_follows_the_fdc_delay_and_time_of_flight() {
    let (ds, m) = world();
    let target = [500.0, 4500.0];
    let sc = with_battery(scenario(NOON, vec![unit("B1", "bulwark_a1", target, 0.0, None)], vec![]), None, None);
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    call(&mut sim, target, FireKind::He, 2);
    let rep = fire_reports(&run_events(&mut sim, 160.0));
    let at = |f: fn(&FireReport) -> bool| rep.iter().filter(|(_, r)| f(r)).map(|(t, _)| secs(*t)).collect::<Vec<_>>();
    let eta = match rep[0].1 {
        FireReport::Accepted { eta_s } => eta_s,
        r => panic!("expected acceptance, got {r:?}"),
    };
    // 75 s FDC + 8 s + 3.6 s/km over 4 km.
    assert!((eta - (75.0 + 8.0 + 3.6 * 4.0)).abs() < 0.5, "eta {eta}");
    let shot = at(|r| matches!(r, FireReport::Shot));
    assert_eq!(shot.len(), 1, "SHOT is announced once per mission");
    assert!((shot[0] - 75.0).abs() < 1.0, "shot at {}", shot[0]);
    let splash = at(|r| matches!(r, FireReport::Splash));
    assert_eq!(splash.len(), 2, "one SPLASH per volley");
    assert!((splash[0] - (eta - 5.0)).abs() < 1.5, "first splash at {}", splash[0]);
    let done = at(|r| matches!(r, FireReport::RoundsComplete));
    assert_eq!(done.len(), 1);
    assert!((done[0] - (eta + 15.0)).abs() < 1.5, "rounds complete at {}", done[0]);
}

#[test]
fn the_battery_is_in_the_picture_with_its_stock() {
    let (ds, m) = world();
    let sc =
        with_battery(scenario(NOON, vec![unit("B1", "bulwark_a1", [500.0, 4500.0], 0.0, None)], vec![]), None, None);
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    let b = sim.perceived(BLUE).fire_support.clone();
    assert_eq!(b.len(), 1);
    assert_eq!((b[0].tubes, b[0].he_rounds, b[0].smoke_rounds), (4, 72, 16));
    assert!(sim.perceived(SideId(1)).fire_support.is_empty());
    call(&mut sim, [500.0, 4500.0], FireKind::He, 1);
    run(&mut sim, 100.0);
    assert_eq!(sim.perceived(BLUE).fire_support[0].he_rounds, 68, "a volley fires one round per tube");
}

#[test]
fn calls_outside_the_battery_envelope_are_refused() {
    let (ds, m) = world();
    let far = [500.0, 500.0 + 30_000.0];
    let near = [500.0, 900.0];
    let ok = [500.0, 4500.0];
    let cases = [
        (far, Some(10), DenyReason::OutOfRange),
        (near, Some(10), DenyReason::TooClose),
        (ok, Some(0), DenyReason::NoAmmo),
    ];
    for (at, he, why) in cases {
        let sc = with_battery(scenario(NOON, vec![unit("B1", "bulwark_a1", ok, 0.0, None)], vec![]), he, None);
        let mut sim = Sim::new(&sc, ds.clone(), m.clone()).unwrap();
        call(&mut sim, at, FireKind::He, 1);
        let rep = fire_reports(&run_events(&mut sim, 5.0));
        assert_eq!(rep, vec![(0, FireReport::Denied(why))]);
    }
    let sc = with_battery(scenario(NOON, vec![unit("B1", "bulwark_a1", ok, 0.0, None)], vec![]), None, None);
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    call(&mut sim, ok, FireKind::He, 1);
    call(&mut sim, ok, FireKind::He, 1);
    let rep = fire_reports(&run_events(&mut sim, 5.0));
    assert!(rep.iter().any(|(_, r)| *r == FireReport::Denied(DenyReason::Busy)));
}

#[test]
fn a_battery_can_be_ceased_but_rounds_in_the_air_still_land() {
    let (ds, m) = world();
    let target = [500.0, 4500.0];
    let sc = with_battery(scenario(NOON, vec![unit("B1", "bulwark_a1", target, 0.0, None)], vec![]), None, None);
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    call(&mut sim, target, FireKind::He, 6);
    run(&mut sim, 80.0); // first volley away at 75 s
    sim.submit_immediate(BLUE, Command::CeaseFire { battery: 0 });
    let rep = fire_reports(&run_events(&mut sim, 120.0));
    assert!(rep.iter().any(|(_, r)| *r == FireReport::Ceased));
    assert_eq!(sim.perceived(BLUE).fire_support[0].he_rounds, 68, "only the first volley was fired");
    assert!(sim.perceived(BLUE).fire_support[0].status == sim::view::BatteryStatus::Ready);
}

/// Along-range error of every burst of a long mission at `range` metres.
fn range_errors(range: f64) -> Vec<f64> {
    let (ds, m) = world();
    let target = [BATTERY[0], BATTERY[1] + range];
    let sc = with_battery(
        scenario(NOON, vec![unit("B1", "falcon_r1", [target[0] + 1000.0, target[1]], 0.0, None)], vec![]),
        Some(400),
        None,
    );
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    call(&mut sim, target, FireKind::He, 6);
    let mut seen: Vec<(u64, [f64; 2])> = Vec::new();
    for _ in 0..(200 * sim::TICK_HZ) {
        sim.step();
        for i in &sim.perceived(BLUE).impacts {
            let key = ((i.time_s / sim::DT).round() as u64, i.pos);
            if !seen.contains(&key) {
                seen.push(key);
            }
        }
    }
    seen.iter().map(|(_, p)| p[1] - target[1]).collect()
}

fn rms(v: &[f64]) -> f64 {
    (v.iter().map(|x| x * x).sum::<f64>() / v.len() as f64).sqrt()
}

#[test]
fn dispersion_grows_with_range() {
    let (near, far) = (range_errors(2500.0), range_errors(5500.0));
    assert_eq!((near.len(), far.len()), (24, 24), "six volleys of four rounds");
    // Range error is 0.4 per cent of range (1σ): about 10 m and 22 m.
    assert!(rms(&near) > 3.0 && rms(&near) < 25.0, "near {}", rms(&near));
    assert!(rms(&far) > rms(&near) * 1.2, "near {} far {}", rms(&near), rms(&far));
}

/// Aim point on top of a single stationary vehicle: outcome after a six-volley mission.
fn vs_vehicle(vehicle: &str, seed: u64) -> (bool, bool) {
    let (ds, m) = world();
    let target = [500.0, 3500.0];
    let mut sc = with_battery(scenario(NOON, vec![], vec![unit("R1", vehicle, target, 0.0, None)]), Some(400), None);
    sc.seed = seed;
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    call(&mut sim, target, FireKind::He, 6);
    run(&mut sim, 180.0);
    let k = (0..sim.truth().len()).find(|&k| sim.truth().callsign[k] == "R1").unwrap();
    let c = sim.truth().condition[k];
    (c != sim_view::Condition::Operational, c == sim_view::Condition::Destroyed)
}

use sim::view as sim_view;

#[test]
fn he_hurts_light_vehicles_far_more_than_tanks() {
    let n: u64 = 12;
    let count = |v: &str| {
        let r: Vec<_> = (0..n).map(|s| vs_vehicle(v, s)).collect();
        (r.iter().filter(|x| x.0).count() as u64, r.iter().filter(|x| x.1).count() as u64)
    };
    let (_, tank_dead) = count("kurgan_b");
    let (recon, recon_dead) = count("falcon_r1");
    assert!(recon_dead > tank_dead, "recon destroyed {recon_dead}/{n}, tanks {tank_dead}/{n}");
    assert!(recon >= n / 2, "recon damaged only {recon}/{n}");
    assert!(tank_dead <= n / 4, "tanks destroyed {tank_dead}/{n}");
}

#[test]
fn bursts_suppress_crews_and_suppression_fades() {
    let (ds, m) = world();
    let target = [500.0, 3500.0];
    let sc = with_battery(scenario(NOON, vec![unit("B1", "bulwark_a1", target, 0.0, None)], vec![]), None, None);
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    call(&mut sim, target, FireKind::He, 1);
    let mut peak: f64 = 0.0;
    for _ in 0..(110 * sim::TICK_HZ) {
        sim.step();
        peak = peak.max(sim.truth().suppression[0]);
    }
    assert!(peak > 0.2, "peak suppression {peak}");
    run(&mut sim, 60.0);
    assert!(sim.truth().suppression[0] < 0.01, "suppression decays: {}", sim.truth().suppression[0]);
}

#[test]
fn near_misses_from_direct_fire_suppress() {
    let (ds, m) = world();
    let sc = scenario(
        NOON,
        vec![unit("B1", "bulwark_a1", [1200.0, 300.0], 0.0, None)],
        vec![unit("R1", "kurgan_b", [1200.0, 2300.0], 180.0, None)],
    );
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    let mut peak: f64 = 0.0;
    for _ in 0..(120 * sim::TICK_HZ) {
        sim.step();
        for k in 0..sim.truth().len() {
            peak = peak.max(sim.truth().suppression[k]);
        }
    }
    assert!(peak > 0.0, "a tank duel suppresses");
}

#[test]
fn artillery_smoke_screens_a_position() {
    let (ds, m) = world();
    let target = [500.0, 3500.0];
    let sc = with_battery(
        scenario(NOON, vec![unit("B1", "bulwark_a1", [target[0] + 600.0, target[1]], 0.0, None)], vec![]),
        None,
        None,
    );
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    call(&mut sim, target, FireKind::Smoke, 1);
    run(&mut sim, 100.0);
    let pic = sim.perceived(BLUE);
    assert!(!pic.smoke.is_empty(), "WP rounds leave clouds");
    assert_eq!(pic.fire_support[0].smoke_rounds, 12);
    assert_eq!(pic.fire_support[0].he_rounds, 72);
}

#[test]
fn a_platoon_under_a_burst_reports_indirect_fire_once() {
    let (ds, m) = world();
    let target = [500.0, 3500.0];
    let sc = with_battery(
        scenario(
            NOON,
            vec![
                unit("1-1", "bulwark_a1", [target[0] + 40.0, target[1]], 0.0, Some("1")),
                unit("1-2", "bulwark_a1", [target[0] + 100.0, target[1]], 0.0, Some("1")),
            ],
            vec![],
        ),
        None,
        None,
    );
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    call(&mut sim, target, FireKind::He, 3);
    let ev = run_events(&mut sim, 140.0);
    let reports: Vec<_> = ev
        .iter()
        .filter_map(|(t, e)| match e {
            Event::PlatoonReport { report: PlatoonReport::IndirectFire { clearing, .. }, .. } => Some((*t, *clearing)),
            _ => None,
        })
        .collect();
    assert_eq!(reports.len(), 1, "one report per cooldown: {reports:?}");
    assert!(!reports[0].1, "a static platoon holds");
}

#[test]
fn fire_support_is_deterministic() {
    let hash = || {
        let (ds, m) = world();
        let target = [500.0, 3500.0];
        let sc = with_battery(scenario(NOON, vec![unit("B1", "bulwark_a1", target, 0.0, None)], vec![]), None, None);
        let mut sim = Sim::new(&sc, ds, m).unwrap();
        call(&mut sim, target, FireKind::He, 3);
        run(&mut sim, 140.0);
        sim.state_hash()
    };
    assert_eq!(hash(), hash());
}

fn with_ai(mut sc: ScenarioDef) -> ScenarioDef {
    sc.sides[0].ai = Some(sim_data::AiPlan { tasks: vec![] });
    sc
}

#[test]
fn the_commander_ai_calls_fire_on_a_fresh_contact_in_range() {
    let (ds, m) = world();
    let sc = with_ai(with_battery(
        scenario(
            NOON,
            vec![unit("1-1", "bulwark_a1", [500.0, 1500.0], 0.0, Some("1"))],
            vec![unit("211", "kurgan_b", [500.0, 3500.0], 180.0, Some("21"))],
        ),
        None,
        None,
    ));
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    let rep = fire_reports(&run_events(&mut sim, 60.0));
    assert!(rep.iter().any(|(_, r)| matches!(r, FireReport::Accepted { .. })), "no mission called: {rep:?}");
}

#[test]
fn the_commander_ai_does_not_call_danger_close() {
    let (ds, m) = world();
    let sc = with_ai(with_battery(
        scenario(
            NOON,
            vec![unit("1-1", "bulwark_a1", [500.0, 1500.0], 0.0, Some("1"))],
            vec![unit("211", "kurgan_b", [500.0, 1650.0], 180.0, Some("21"))],
        ),
        None,
        None,
    ));
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    let rep = fire_reports(&run_events(&mut sim, 60.0));
    assert!(rep.is_empty(), "danger close call: {rep:?}");
}

/// Bursts seen by `sim`'s Blue picture while stepping `seconds`, as positions (deduplicated).
fn observed_bursts(sim: &mut Sim, seconds: f64) -> Vec<[f64; 2]> {
    let mut out: Vec<(u64, [f64; 2])> = Vec::new();
    for _ in 0..seconds as usize {
        run(sim, 1.0);
        for i in &sim.perceived(BLUE).impacts {
            let key = (i.time_s * 1000.0) as u64;
            if !out.iter().any(|(k, p)| *k == key && *p == i.pos) {
                out.push((key, i.pos));
            }
        }
    }
    out.into_iter().map(|(_, p)| p).collect()
}

fn centroid(p: &[[f64; 2]]) -> [f64; 2] {
    let n = p.len() as f64;
    [p.iter().map(|q| q[0]).sum::<f64>() / n, p.iter().map(|q| q[1]).sum::<f64>() / n]
}

#[test]
fn adjusting_fire_cancels_the_systematic_error() {
    let target = [500.0, 5000.0];
    let (mut before, mut after) = (0.0, 0.0);
    for seed in 0..8u64 {
        let (ds, m) = world();
        let mut sc = with_battery(
            scenario(NOON, vec![unit("O1", "bulwark_a1", [500.0, 3900.0], 0.0, None)], vec![]),
            None,
            None,
        );
        sc.seed = seed;
        let mut sim = Sim::new(&sc, ds, m).unwrap();
        call(&mut sim, target, FireKind::He, 1);
        let first = observed_bursts(&mut sim, 130.0);
        assert_eq!(first.len(), 4, "one volley of four rounds");
        let c = centroid(&first);
        before += dist(c, target);
        let shift = [target[0] - c[0], target[1] - c[1]];
        sim.submit_immediate(BLUE, Command::AdjustFire { battery: 0, shift, volleys: 1 });
        let ev = run_events(&mut sim, 1.0);
        let eta = fire_reports(&ev).iter().find_map(|(_, r)| match r {
            FireReport::Accepted { eta_s } => Some(*eta_s),
            _ => None,
        });
        // 20 s adjustment delay + 8 s + 3.6 s/km over 4.5 km: well under a new call's 75 s FDC delay.
        assert!(eta.is_some_and(|e| (e - (20.0 + 8.0 + 3.6 * 4.5)).abs() < 1.0), "eta {eta:?}");
        let second = observed_bursts(&mut sim, 60.0);
        assert_eq!(second.len(), 4);
        after += dist(centroid(&second), target);
    }
    assert!(after < before * 0.6, "mean centroid error before {} after {}", before / 8.0, after / 8.0);
}

#[test]
fn adjusting_needs_a_previous_mission_and_an_idle_battery() {
    let (ds, m) = world();
    let sc =
        with_battery(scenario(NOON, vec![unit("O1", "bulwark_a1", [500.0, 3900.0], 0.0, None)], vec![]), None, None);
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    sim.submit_immediate(BLUE, Command::AdjustFire { battery: 0, shift: [10.0, 0.0], volleys: 1 });
    let rep = fire_reports(&run_events(&mut sim, 1.0));
    assert!(matches!(rep[0].1, FireReport::Denied(DenyReason::NoMission)), "{rep:?}");
    call(&mut sim, [500.0, 5000.0], FireKind::He, 2);
    run(&mut sim, 1.0);
    sim.submit_immediate(BLUE, Command::AdjustFire { battery: 0, shift: [10.0, 0.0], volleys: 1 });
    let rep = fire_reports(&run_events(&mut sim, 1.0));
    assert!(matches!(rep[0].1, FireReport::Denied(DenyReason::Busy)), "{rep:?}");
}

#[test]
fn the_commander_ai_screens_with_smoke_on_entering_battle() {
    let (ds, m) = world();
    let mut sc = with_battery(
        scenario(
            NOON,
            vec![unit("1-1", "bulwark_a1", [500.0, 1500.0], 0.0, Some("1"))],
            vec![unit("211", "kurgan_b", [500.0, 3500.0], 180.0, Some("21"))],
        ),
        Some(0),
        Some(12),
    );
    sc.objectives = vec![sim_data::ObjectiveDef { name: "OBJ".into(), centre: [500.0, 3000.0], radius_m: 200.0 }];
    sc.sides[0].ai = Some(sim_data::AiPlan {
        tasks: vec![sim_data::AiTask {
            platoons: vec!["1".into()],
            mission: sim_data::Mission::Attack {
                axis: vec![[500.0, 2200.0], [500.0, 3000.0]],
                objective: "OBJ".into(),
            },
            start_s: 20.0,
            release_on_contact: false,
            latest_s: None,
        }],
    });
    let mut sim = Sim::new(&sc, ds, m).unwrap();
    let ev = run_events(&mut sim, 90.0);
    let smoke = ev.iter().any(|(_, e)| matches!(e, Event::FireSupport { report: FireReport::Accepted { .. }, .. }));
    assert!(smoke, "no smoke mission called on entering battle (battery has no HE)");
}
