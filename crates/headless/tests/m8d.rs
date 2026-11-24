//! M8d exit (plan docs/plans/M8D-ENGINEERS.md §8 #9): in `m8d-deliberate-breach` Blue attacks the
//! uplands village through a minefield and wire belt. Played by both commander AIs, Blue's plough
//! and roller platoons breach it and win; without the kits the attack stalls at the belt (no
//! acceptable bypass) and Red holds to the time limit (seed 1; the batch over seeds 1-4 is in
//! DESIGN.md §13.1).

use std::path::PathBuf;
use std::sync::Arc;

use sim::{Event, PlatoonReport, SideId, Sim};
use sim_data::{DataSet, ScenarioDef};

/// Play to the end: the winner and Blue's reports.
fn play(sc: &ScenarioDef, ds: &Arc<DataSet>, dir: &std::path::Path) -> (Option<SideId>, Vec<PlatoonReport>) {
    let m = Arc::new(terragen::workflow::ensure_map(dir, ds, &sc.map, |_| {}).expect("map"));
    let mut sim = Sim::new(sc, ds.clone(), m).expect("sim");
    sim.set_ai_enabled(SideId(0), true);
    let limit = sc.victory.as_ref().unwrap().time_limit_s + 60.0;
    let (mut winner, mut reports) = (None, Vec::new());
    for _ in 0..(limit * sim::TICK_HZ as f64) as u64 {
        for e in sim.step() {
            match e {
                Event::ScenarioEnd { winner: w, .. } => winner = Some(w),
                Event::PlatoonReport { side: SideId(0), report, .. } => reports.push(report),
                _ => {}
            }
        }
        if sim.ended() {
            break;
        }
    }
    (winner.expect("the scenario ended"), reports)
}

#[test]
fn the_plough_and_roller_platoons_breach_the_belt_and_win() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data");
    let ds = Arc::new(DataSet::load(&dir).expect("data"));
    let mut sc = DataSet::load_scenario(&dir, "m8d-deliberate-breach").expect("scenario");
    sc.seed = 1;
    let (winner, reports) = play(&sc, &ds, &dir);
    let lanes = reports.iter().filter(|r| matches!(r, PlatoonReport::LaneOpen { .. })).count();
    assert!(lanes >= 1, "a lane was opened: {reports:?}");
    assert_eq!(winner, Some(SideId(0)), "Blue wins through the lanes");

    // The same attack without the kits: no lane, no acceptable bypass, the attack stalls.
    for u in &mut sc.sides[0].units {
        u.kit = None;
    }
    let (winner, reports) = play(&sc, &ds, &dir);
    assert!(!reports.iter().any(|r| matches!(r, PlatoonReport::LaneOpen { .. })));
    assert!(reports.contains(&PlatoonReport::NoBypass), "{reports:?}");
    assert_eq!(winner, Some(SideId(1)), "Red holds the village");
}
