//! PT22 (playtest 2026-11-06): a road march took HASTY ATTACK while Red fired a handful of rounds.
//! Red fell back to an alternate position that could not see the village and never returned.
//! ATP 3-20.15 ch. 5: an alternate position covers the same ground; FM 3-90: counterattack to
//! retake lost ground.

use std::path::PathBuf;
use std::sync::Arc;

use sim::{EndReason, Event, SideId, Sim};
use sim_data::{DataSet, Mission};

#[test]
fn red_fights_for_the_village_from_a_position_that_sees_it() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data");
    let ds = Arc::new(DataSet::load(&dir).expect("data"));
    let mut sc = DataSet::load_scenario(&dir, "cw-hasty-attack").expect("scenario");
    sc.seed = 1;
    let m = Arc::new(terragen::workflow::ensure_map(&dir, &ds, &sc.map, |_| {}).expect("map"));

    // The alternate sees a good part of the objective, vegetation counted.
    let obj = sc.objectives[0].clone();
    let alternate = sc.sides[1]
        .ai
        .as_ref()
        .and_then(|ai| {
            ai.tasks.iter().find_map(|t| match &t.mission {
                Mission::Defend { alternates, counterattack: Some(_), .. } => alternates.last().cloned(),
                _ => None,
            })
        })
        .expect("Red defends with an alternate and a counter-attack objective");
    let g = terrain::ground::Ground::new(&m, &ds.terrain);
    let points: Vec<[f64; 2]> = (-2..=2)
        .flat_map(|i| (-2..=2).map(move |j| (i, j)))
        .map(|(i, j)| [obj.centre[0] + i as f64 * 120.0, obj.centre[1] + j as f64 * 120.0])
        .collect();
    let seen = points
        .iter()
        .filter(|&&p| {
            let s = terrain::los::sight(&g, alternate.centre, 2.8, p, &[1.2, 1.9, 2.4]);
            s.any() && s.transmittance > 0.3
        })
        .count();
    assert!(seen * 3 >= points.len(), "the alternate sees {seen} of {} points", points.len());

    let mut sim = Sim::new(&sc, ds.clone(), m.clone()).expect("sim");
    sim.set_ai_enabled(SideId(0), true);
    let mut reason = None;
    for _ in 0..((sc.victory.as_ref().unwrap().time_limit_s + 60.0) * sim::TICK_HZ as f64) as u64 {
        for e in sim.step() {
            if let Event::ScenarioEnd { reason: r, .. } = e {
                reason = Some(r);
            }
        }
        if sim.ended() {
            break;
        }
    }
    assert!(
        !matches!(reason, Some(EndReason::ObjectiveSeized { .. })),
        "the village was not taken uncontested: {reason:?}"
    );
}
