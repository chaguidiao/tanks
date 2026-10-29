//! M8h1b towns in play (plan `docs/plans/M8H1B-TOWNS.md` §5), on the `town` map of the
//! `m8h-town` scenario: tank platoons route round the town unless told BUILT-UP ENTER, a column
//! sent through it gets across (Z4 re-check), and a slab block takes at most four squads.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use sim::view::{BuiltUp, PlatoonId};
use sim::{Addressee, Command, SideId, Sim};
use sim_data::{BuildingClass, DataSet, Formation, ScenarioDef, Technique, UnitDef};

const BLUE: SideId = SideId(0);
/// Two points beyond the town (its square is at 4200, 3400), south and north of it.
const SOUTH: [f64; 2] = [4200.0, 2500.0];
const NORTH: [f64; 2] = [4200.0, 4400.0];

fn data() -> (PathBuf, Arc<DataSet>) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data");
    let ds = DataSet::load(&dir).expect("data");
    (dir, Arc::new(ds))
}

fn map(dir: &Path, ds: &DataSet) -> Arc<terrain::BakedMap> {
    let sc = DataSet::load_scenario(dir, "m8h-town").expect("scenario");
    Arc::new(terragen::workflow::ensure_map(dir, ds, &sc.map, |_| {}).expect("map"))
}

fn unit(cs: &str, vehicle: &str, at: [f64; 2], heading_deg: f64, platoon: &str) -> UnitDef {
    UnitDef {
        callsign: cs.into(),
        vehicle: vehicle.into(),
        position: at,
        heading_deg,
        platoon: Some(platoon.into()),
        squad: None,
        mounted_in: None,
    }
}

fn squad(cs: &str, at: [f64; 2], platoon: &str) -> UnitDef {
    UnitDef {
        callsign: cs.into(),
        vehicle: String::new(),
        position: at,
        heading_deg: 0.0,
        platoon: Some(platoon.into()),
        squad: Some("rifle_squad_b".into()),
        mounted_in: None,
    }
}

/// The scenario with its units replaced: Blue `blue`, Red one tank far away, no AI tasks.
fn sim(dir: &Path, ds: &Arc<DataSet>, m: &Arc<terrain::BakedMap>, blue: Vec<UnitDef>) -> Sim {
    let mut sc: ScenarioDef = DataSet::load_scenario(dir, "m8h-town").expect("scenario");
    sc.victory = None;
    sc.sides[0].units = blue;
    sc.sides[0].ai = None;
    sc.sides[1].units = vec![unit("211", "kurgan_b", [7600.0, 7600.0], 225.0, "21")];
    sc.sides[1].ai = None;
    let mut s = Sim::new(&sc, ds.clone(), m.clone()).expect("sim");
    s.set_ai_enabled(BLUE, false);
    s.set_ai_enabled(SideId(1), false);
    s
}

fn tanks(dir: &Path, ds: &Arc<DataSet>, m: &Arc<terrain::BakedMap>) -> Sim {
    let blue = (0..4)
        .map(|i| unit(&format!("1-{}", i + 1), "bulwark_a1", [SOUTH[0] - 60.0 + 40.0 * i as f64, SOUTH[1]], 0.0, "1"))
        .collect();
    sim(dir, ds, m, blue)
}

fn platoon(sim: &Sim, cs: &str) -> PlatoonId {
    sim.perceived(BLUE).platoons.iter().find(|p| p.callsign == cs).expect(cs).id
}

fn move_north(sim: &mut Sim, formation: Option<Formation>) {
    let p = platoon(sim, "1");
    sim.submit_immediate(
        BLUE,
        Command::PlatoonMove {
            platoon: p,
            destination: NORTH,
            via: vec![],
            formation,
            technique: Technique::Traveling,
            speed_limit_mps: None,
        },
    );
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

fn run(sim: &mut Sim, s: f64) {
    for _ in 0..(s * sim::TICK_HZ as f64) as u64 {
        sim.step();
    }
}

/// Length of the planned route on built-up ground (`sim::builtup::mask`: built-up land cover
/// and 30 m round buildings, the doctrine margin), sampled every 4 m.
fn route_in_town(sim: &mut Sim, m: &terrain::BakedMap, ds: &DataSet) -> f64 {
    run(sim, 3.0);
    let v = sim.perceived(BLUE);
    let r = &v.platoon(platoon(sim, "1")).unwrap().route;
    assert!(!r.is_empty(), "a route is planned");
    let mask = sim::builtup::mask(m, &ds.terrain, 30.0);
    let cell = m.manifest.cell_size_m;
    let mut on = 0.0;
    for w in r.windows(2) {
        let n = (dist(w[0], w[1]) / 4.0).ceil().max(1.0) as usize;
        for k in 0..n {
            let t = (k as f64 + 0.5) / n as f64;
            let q = [w[0][0] + (w[1][0] - w[0][0]) * t, w[0][1] + (w[1][1] - w[0][1]) * t];
            if mask.get((q[0] / cell) as usize, (q[1] / cell) as usize) {
                on += dist(w[0], w[1]) / n as f64;
            }
        }
    }
    on
}

#[test]
fn tanks_route_round_the_town_unless_told_to_enter() {
    let (dir, ds) = data();
    let m = map(&dir, &ds);
    let mut s = tanks(&dir, &ds, &m);
    move_north(&mut s, None);
    let round = route_in_town(&mut s, &m, &ds);
    let mut s = tanks(&dir, &ds, &m);
    let p = platoon(&s, "1");
    s.submit_immediate(BLUE, Command::SetBuiltUp { platoon: p, built_up: BuiltUp::Enter });
    move_north(&mut s, None);
    let through = route_in_town(&mut s, &m, &ds);
    println!("route on built-up ground: default {round:.0} m, ENTER {through:.0} m");
    assert!(round < 50.0, "by default the route keeps off built-up ground ({round:.0} m on it)");
    assert!(through > 300.0, "BUILT-UP ENTER drives through the town ({through:.0} m on built-up ground)");
}

#[test]
fn column_and_wedge_get_through_the_town() {
    // Z4 re-check on a town: BUILT-UP ENTER, south edge to north edge (1.9 km, 1.2 km of it in
    // the town). Formations follow the route by arc length (H10), so a wedge folds into the
    // street rather than re-planning trailing slots.
    let (dir, ds) = data();
    let m = map(&dir, &ds);
    for formation in [Formation::Column, Formation::Wedge] {
        let mut s = tanks(&dir, &ds, &m);
        let p = platoon(&s, "1");
        s.submit_immediate(BLUE, Command::SetBuiltUp { platoon: p, built_up: BuiltUp::Enter });
        move_north(&mut s, Some(formation));
        let members: Vec<usize> = (0..s.truth().len()).filter(|&k| s.truth().side[k] == BLUE).collect();
        let mut arrived = None;
        for t in 0..900 {
            run(&mut s, 1.0);
            // The tail stays its spacing behind the leader: arrived when the leader is in.
            if members.iter().any(|&k| dist(s.truth().transform[k].pos, NORTH) < 50.0)
                && s.perceived(BLUE).platoon(p).is_some_and(|v| v.route.is_empty())
            {
                arrived = Some(t + 1);
                break;
            }
        }
        println!("{formation:?} through the town: {arrived:?} s");
        assert!(arrived.is_some_and(|t| t < 600), "{formation:?} crosses the town in under 10 minutes ({arrived:?} s)");
    }
}

#[test]
fn a_slab_block_takes_at_most_four_squads() {
    let (dir, ds) = data();
    let m = map(&dir, &ds);
    let slab = m
        .buildings
        .iter()
        .filter(|b| b.class == BuildingClass::Concrete && !b.entrances.is_empty())
        .max_by(|a, b| a.footprint_m2.total_cmp(&b.footprint_m2))
        .expect("a slab block");
    assert!(slab.footprint_m2 > 500.0, "a large slab ({} m²)", slab.footprint_m2);
    // Five squads 15 m outside the slab's first entrance.
    let (e, c) = (slab.entrances[0], slab.centroid);
    let (dx, dy) = ((e[0] - c[0]) as f64, (e[1] - c[1]) as f64);
    let l = (dx * dx + dy * dy).sqrt();
    let out = [e[0] as f64 + dx / l * 15.0, e[1] as f64 + dy / l * 15.0];
    let blue = (0..5).map(|i| squad(&format!("1-{}", i + 1), [out[0] + 3.0 * i as f64, out[1]], "1-D")).collect();
    let mut s = sim(&dir, &ds, &m, blue);
    let p = platoon(&s, "1-D");
    s.submit_immediate(BLUE, Command::OccupyBuilding { to: Addressee::Platoon(p), building: slab.id });
    run(&mut s, 600.0);
    let w = s.truth();
    let inside = (0..w.len()).filter(|&k| w.squad[k].as_ref().is_some_and(|q| q.in_building == Some(slab.id))).count();
    assert_eq!(inside, 4, "the slab holds four squads, not the 36 its floor area would");
}
