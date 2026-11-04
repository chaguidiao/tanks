//! M8c exit (TODO M8c "a long scenario where ammunition decides the outcome"): in
//! `m8c-long-defence` the Blue platoons start with a quarter of their rounds. Played by both
//! commander AIs, Blue holds with its supply trucks, supply point and recovery vehicle, and loses
//! with its rounds spent when they are taken away (seed 1; the batch over seeds 1-6 is in
//! DESIGN.md §13.1).

use std::path::PathBuf;
use std::sync::Arc;

use sim::{Event, SideId, Sim};
use sim_data::{DataSet, ScenarioDef};

/// Play to the end; the winner and Blue's main-gun rounds left in its tanks.
fn play(sc: &ScenarioDef, ds: &Arc<DataSet>, dir: &std::path::Path) -> (Option<SideId>, u32, usize) {
    let m = Arc::new(terragen::workflow::ensure_map(dir, ds, &sc.map, |_| {}).expect("map"));
    let mut sim = Sim::new(sc, ds.clone(), m).expect("sim");
    sim.set_ai_enabled(SideId(0), true);
    let limit = sc.victory.as_ref().unwrap().time_limit_s + 60.0;
    let (mut winner, mut resupplied) = (None, 0);
    for _ in 0..(limit * sim::TICK_HZ as f64) as u64 {
        for e in sim.step() {
            match e {
                Event::ScenarioEnd { winner: w, .. } => winner = Some(w),
                Event::PlatoonReport { side: SideId(0), report: sim::PlatoonReport::Resupplied { .. }, .. } => {
                    resupplied += 1
                }
                _ => {}
            }
        }
        if sim.ended() {
            break;
        }
    }
    let w = sim.truth();
    let rounds = (0..w.len())
        .filter(|&k| w.side[k] == SideId(0) && w.cargo[k].is_none() && w.alive(k))
        .filter_map(|k| w.gun[k].as_ref())
        .map(|g| g.rounds())
        .sum();
    (winner.expect("the scenario ended"), rounds, resupplied)
}

#[test]
fn in_the_long_defence_ammunition_decides() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data");
    let ds = Arc::new(DataSet::load(&dir).expect("data"));
    let mut sc = DataSet::load_scenario(&dir, "m8c-long-defence").expect("scenario");
    sc.seed = 1;
    let (winner, _, resupplied) = play(&sc, &ds, &dir);
    assert_eq!(winner, Some(SideId(0)), "Blue holds with its logistics");
    assert!(resupplied > 0, "and the commander used them");

    // The same fight without trucks, supply point and recovery vehicle.
    let blue = &mut sc.sides[0];
    blue.supply_points.clear();
    blue.units.retain(|u| !u.callsign.starts_with("SUP") && !u.callsign.starts_with('R'));
    let (winner, rounds, _) = play(&sc, &ds, &dir);
    assert_eq!(winner, Some(SideId(1)), "Red wins once Blue's rounds run out");
    assert!(rounds < 40, "Blue's surviving tanks are nearly out of rounds: {rounds}");
}
