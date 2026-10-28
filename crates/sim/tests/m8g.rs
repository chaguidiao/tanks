//! M8g1 infantry plans (TODO G8): plans for dismount platoons and the DISMOUNT / MOUNT /
//! OCCUPY BUILDING plan actions.

mod common;

use ai::picture::{Plan, PlanStep, PlatoonId, StepAction};
use common::*;
use sim::{Command, Event, PlatoonReport, SideId, Sim};

const BLUE: SideId = SideId(0);
const CARRIERS: PlatoonId = PlatoonId(1);
const DISMOUNTS: PlatoonId = PlatoonId(2);

/// Blue: an M2 platoon of two carrying a squad each; a one-storey masonry house of 24 × 14 m
/// (room for both squads) 1 km north. Red far away and passive.
fn mech_sim() -> Sim {
    let ds = data();
    let m = map_with(&ds, 400, 800, "grassland", |p| {
        p.features.buildings.push(building(1, 640.0, 1400.0, 664.0, 1414.0, sim_data::BuildingClass::Masonry));
    });
    let blue = vec![
        unit("1-1", "ward_m2", [600.0, 400.0], 0.0, Some("1")),
        unit("1-2", "ward_m2", [700.0, 400.0], 0.0, Some("1")),
        squad("1-1A", "rifle_squad_b", [0.0, 0.0], None, Some("1-1")),
        squad("1-2A", "rifle_squad_b", [0.0, 0.0], None, Some("1-2")),
    ];
    let red = vec![unit("211", "kurgan_b", [800.0, 3000.0], 180.0, Some("21"))];
    let mut sim = Sim::new(&scenario(10 * 3600, blue, red), ds, m).unwrap();
    sim.set_ai_enabled(SideId(1), false);
    sim
}

fn plan(steps: Vec<PlanStep>) -> Plan {
    Plan { steps }
}

fn cp(id: u16, at: [f64; 2], actions: Vec<StepAction>) -> PlanStep {
    let mut s = PlanStep::new(id, at);
    s.actions = actions;
    s
}

fn reported(ev: &[Event], platoon: PlatoonId, want: &PlatoonReport) -> bool {
    ev.iter().any(|e| matches!(e, Event::PlatoonReport { platoon: p, report, .. } if *p == platoon && report == want))
}

#[test]
fn carriers_dismount_at_a_checkpoint_and_the_squads_run_their_own_plan() {
    let mut sim = mech_sim();
    // The squads' plan is sent while they ride: it starts once they are on the ground.
    let house = [652.0, 1407.0];
    let squads = plan(vec![cp(1, house, vec![StepAction::OccupyBuilding])]);
    sim.submit_immediate(BLUE, Command::SetPlan { platoon: DISMOUNTS, plan: squads, start: 1 });
    let carriers = plan(vec![cp(1, [650.0, 1000.0], vec![StepAction::Dismount]), cp(2, [650.0, 700.0], vec![])]);
    sim.submit_immediate(BLUE, Command::SetPlan { platoon: CARRIERS, plan: carriers, start: 1 });
    let mut ev = Vec::new();
    let (mut reached, mut left_cp1_mounted) = (false, false);
    // About 10 minutes on foot from CP 1 to the house at 0.67 m/s.
    for _ in 0..300 {
        ev.extend(run(&mut sim, 5.0));
        let w = sim.truth();
        reached |= w.transform[0].pos[1] > 950.0;
        // Nobody drives on to CP 2 before the ramp drill is over.
        left_cp1_mounted |= reached && w.mounted(2) && w.transform[0].pos[1] < 940.0;
    }
    assert!(!left_cp1_mounted);
    assert!(reported(&ev, CARRIERS, &PlatoonReport::Dismounted), "dismounted at CP 1");
    assert!(reported(&ev, CARRIERS, &PlatoonReport::PlanComplete), "carriers went on to CP 2");
    let w = sim.truth();
    assert!(dist(w.transform[0].pos, [650.0, 700.0]) < 120.0, "carrier at CP 2");
    for k in [2, 3] {
        assert_eq!(w.squad[k].as_ref().unwrap().in_building, Some(1), "squad {k} in the house");
    }
    assert!(reported(&ev, DISMOUNTS, &PlatoonReport::PlanComplete), "the squads' plan completes");
}

#[test]
fn a_mount_action_holds_the_carriers_until_the_squads_are_aboard() {
    let mut sim = mech_sim();
    sim.submit_immediate(BLUE, Command::Dismount { platoon: CARRIERS });
    run(&mut sim, 60.0);
    assert!(!sim.truth().mounted(2) && !sim.truth().mounted(3));
    // The squads are sent away; the carriers' plan calls them back and drives off with them.
    sim.submit_immediate(
        BLUE,
        Command::PlatoonMove {
            platoon: DISMOUNTS,
            destination: [650.0, 600.0],
            via: vec![],
            formation: None,
            technique: sim_data::Technique::Traveling,
            speed_limit_mps: None,
        },
    );
    run(&mut sim, 20.0);
    let carriers = plan(vec![cp(1, [650.0, 420.0], vec![StepAction::Mount]), cp(2, [650.0, 1000.0], vec![])]);
    sim.submit_immediate(BLUE, Command::SetPlan { platoon: CARRIERS, plan: carriers, start: 1 });
    let mut ev = Vec::new();
    let mut drove_without = false;
    for _ in 0..60 {
        ev.extend(run(&mut sim, 5.0));
        let w = sim.truth();
        if w.transform[0].pos[1] > 520.0 && !(w.mounted(2) && w.mounted(3)) {
            drove_without = true;
        }
    }
    assert!(!drove_without, "the carriers wait for their squads");
    assert!(reported(&ev, DISMOUNTS, &PlatoonReport::Mounted));
    assert!(reported(&ev, CARRIERS, &PlatoonReport::PlanComplete));
    let w = sim.truth();
    assert!(w.mounted(2) && w.mounted(3));
    assert!(dist(w.transform[0].pos, [650.0, 1000.0]) < 120.0, "carrier at CP 2");
    let view = sim.perceived(BLUE);
    assert!(view.platoon(DISMOUNTS).unwrap().plan.is_none(), "no order left for the mounted squads");
}

#[test]
fn a_dismount_platoon_follows_a_plan_through_checkpoints() {
    let mut sim = mech_sim();
    sim.submit_immediate(BLUE, Command::Dismount { platoon: CARRIERS });
    run(&mut sim, 60.0);
    let squads = plan(vec![cp(1, [500.0, 600.0], vec![StepAction::Report]), cp(2, [650.0, 750.0], vec![])]);
    sim.submit_immediate(BLUE, Command::SetPlan { platoon: DISMOUNTS, plan: squads, start: 1 });
    let ev = run(&mut sim, 900.0);
    assert!(reported(&ev, DISMOUNTS, &PlatoonReport::AtCheckpoint { n: 1, waiting: false }));
    assert!(reported(&ev, DISMOUNTS, &PlatoonReport::PlanComplete));
    let w = sim.truth();
    for k in [2, 3] {
        assert!(dist(w.transform[k].pos, [650.0, 750.0]) < 40.0, "squad {k} at CP 2");
    }
}
