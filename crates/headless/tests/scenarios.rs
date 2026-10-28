//! M7e: the v1.0 scenario set loads, plays and is deterministic. Each scenario runs headless for a
//! few simulated minutes twice (maps regenerate from recipe and seed if `data/maps` is empty);
//! the state hashes must match. Balance is not asserted here: see DESIGN.md §13.1 for the
//! tuning runs (`tanks-headless run --scenario <id> --seed <n> --summary`).

use std::process::Command;

const SCENARIOS: [&str; 6] = [
    "cw-village-defence",
    "cw-night-delay",
    "cw-hasty-attack",
    "mod-fog-meeting",
    "mod-dusk-attack",
    "mod-delay-in-depth",
];

fn run(id: &str) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_tanks-headless"))
        .args(["run", "--scenario", id, "--seconds", "240"])
        .output()
        .expect("run tanks-headless");
    assert!(out.status.success(), "{id}: {}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines().find(|l| l.starts_with("state hash")).unwrap_or_else(|| panic!("{id}: no hash in\n{text}")).to_owned()
}

#[test]
fn the_v1_scenarios_load_play_and_replay_to_the_same_state() {
    for id in SCENARIOS {
        let (a, b) = (run(id), run(id));
        assert_eq!(a, b, "{id} is not deterministic");
    }
}

#[test]
fn the_ambush_demo_plays_and_replays_to_the_same_state() {
    // M8b-b tech demo (docs/plans/M8B-AT-TEAMS.md §10.8): one tank driven down the street past the
    // squads in the houses; deterministic like the v1 set.
    let play = || {
        let out = Command::new(env!("CARGO_BIN_EXE_tanks-headless"))
            .args(["run", "--scenario", "m8b-ambush", "--move", "4300,4290", "--seconds", "300"])
            .output()
            .expect("run tanks-headless");
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        text.lines().find(|l| l.starts_with("state hash")).expect("hash").to_owned()
    };
    assert_eq!(play(), play());
}
