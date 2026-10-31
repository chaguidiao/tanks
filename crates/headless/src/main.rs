//! `tanks-headless`: map generation/baking, debug renders, headless sim runs.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use sim_data::DataSet;
use terragen::workflow;
use terrain::{AuthoringPackage, BakedMap, shade};

const USAGE: &str = "usage: tanks-headless <command> [options]

  gen     --recipe <preset> --seed <n> [--size <m>] [--id <map-id>]   generate + bake a map
  bake    --map <id>                                                   bake an authoring package
  render  --map <id> --layer <tint|shade|slope|traffic|landcover> [--class <vehicle>] [--downsample <n>] --out <png>
  info    --map <id>                                                   print map statistics
  probe   --map <id> --at <x,y[;x,y..]>                                terrain at points
  run     --scenario <id> [--seconds <s>] [--move <x,y>] [--seed <n>] [--summary]
                                                                       run a scenario headless (--summary: one CSV line;
                                                                       default --seconds: time limit + 60)
  detect  [--observer <vehicle|squad>] [--squad <id>] [--posture moving,halted,dug_in] [--ranges m,..] [--hour h]
          [--cover <landcover>] [--year y] [--trials n] [--out <csv>]   infantry time-to-detect by posture
  firefight [--shooter <squad>] [--target <squad>] [--weapons all|<name part>,..] [--posture halted,..]
          [--ranges m,..] [--seconds s] [--year y] [--hour h] [--cover <landcover>] [--trials n] [--mutual]
          [--out <csv>]                                                small-arms validation: rounds per casualty
                                                                       (--mutual: the target fires back; exchange ratio)
  replay  --file <replay.ron>                                          re-simulate a replay, verify its hash
  bench   [--samples <n>] [--ticks <n>] [--csv]                        timings: LOS, viewshed, battle-position search, sim throughput
  duel    [--shooter <vehicle|squad>] [--target <id>] [--ranges 500,1000,..] [--trials <n>] [--aspect front|side]
          [--year <y>] [--out <csv>]                                   Monte-Carlo duel validation harness (a squad
                                                                       shooter fires its rockets or missiles, set up
                                                                       and with its ambush already sprung)

Options: --data <dir> (default: ./data or $TANKS_DATA)";

struct Args(Vec<String>);

impl Args {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.iter().position(|a| a == key).and_then(|i| self.0.get(i + 1)).map(String::as_str)
    }
    fn req(&self, key: &str) -> Result<&str, String> {
        self.get(key).ok_or_else(|| format!("missing {key}"))
    }
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = argv.first().cloned() else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };
    match run(&cmd, Args(argv)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cmd: &str, a: Args) -> Result<(), String> {
    let data =
        a.get("--data").map(PathBuf::from).or_else(sim_data::find_data_dir).ok_or("cannot find data/ directory")?;
    let ds = DataSet::load(&data).map_err(|e| e.to_string())?;
    let log = |s: &str| eprintln!("{s}");
    match cmd {
        "gen" => {
            let preset = a.req("--recipe")?;
            let seed: u64 = a.req("--seed")?.parse().map_err(|_| "bad --seed")?;
            let size: f64 = a.get("--size").unwrap_or("8000").parse().map_err(|_| "bad --size")?;
            let id = a.get("--id").map(str::to_owned).unwrap_or_else(|| format!("{preset}-{seed}"));
            let t = Instant::now();
            let pkg = workflow::generate_map(&data, &ds, preset, &id, seed, size, log).map_err(|e| e.to_string())?;
            eprintln!("generated in {:.1} s", t.elapsed().as_secs_f64());
            let t = Instant::now();
            let baked = workflow::bake_map(&data, &ds, &pkg).map_err(|e| e.to_string())?;
            eprintln!("baked in {:.1} s", t.elapsed().as_secs_f64());
            print_info(&baked);
        }
        "bake" => {
            let id = a.req("--map")?;
            let pkg = AuthoringPackage::load(&workflow::map_dir(&data, id)).map_err(|e| e.to_string())?;
            let t = Instant::now();
            let baked = workflow::bake_map(&data, &ds, &pkg).map_err(|e| e.to_string())?;
            eprintln!("baked in {:.1} s", t.elapsed().as_secs_f64());
            print_info(&baked);
        }
        "info" => {
            let id = a.req("--map")?;
            let baked = BakedMap::load(&workflow::map_dir(&data, id).join("baked")).map_err(|e| e.to_string())?;
            print_info(&baked);
        }
        "render" => {
            let id = a.req("--map")?;
            let out = PathBuf::from(a.req("--out")?);
            let baked = BakedMap::load(&workflow::map_dir(&data, id).join("baked")).map_err(|e| e.to_string())?;
            let (w, h) = (baked.elevation.z.w, baked.elevation.z.h);
            let img = match a.req("--layer")? {
                "tint" => {
                    let mut t = shade::elevation_tint(&baked.elevation);
                    let s = shade::hillshade(&baked.elevation);
                    for (p, q) in t.iter_mut().zip(&s) {
                        *p = ((*p as u32 * *q as u32) / 200).min(255) as u8;
                    }
                    t
                }
                "shade" => shade::hillshade(&baked.elevation),
                "slope" => shade::slope_view(&baked.elevation),
                "landcover" => shade::landcover_view(&baked.landcover, &ds.terrain),
                "traffic" => {
                    let class = a.get("--class").unwrap_or("bulwark_a1");
                    let m = baked.mobility(class).ok_or_else(|| format!("no mobility layer '{class}'"))?;
                    let reference = (m.class.cross_country_speed / terrain::mobility::SPEED_QUANTUM) as u8;
                    shade::trafficability_view(&m.speed, reference)
                }
                other => return Err(format!("unknown layer '{other}'")),
            };
            if let Some(dir) = out.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            let ds_n: usize = a.get("--downsample").unwrap_or("1").parse().map_err(|_| "bad --downsample")?;
            let (img, w, h) = downsample(&img, w, h, ds_n.max(1));
            terrain::png::write_rgba(&out, w, h, &img).map_err(|e| e.to_string())?;
            eprintln!("wrote {}", out.display());
        }
        "run" => run_scenario(&data, &ds, &a)?,
        "probe" => {
            let id = a.req("--map")?;
            let baked = BakedMap::load(&workflow::map_dir(&data, id).join("baked")).map_err(|e| e.to_string())?;
            for xy in a.req("--at")?.split(';') {
                let (x, y) = xy.split_once(',').ok_or("--at expects x,y[;x,y]")?;
                let (x, y): (f64, f64) = (x.parse().map_err(|_| "bad x")?, y.parse().map_err(|_| "bad y")?);
                let (i, j) = baked.elevation.cell_of(x, y);
                let lc = ds.terrain.landcover(baked.landcover.get(i, j));
                println!(
                    "({x:.0}, {y:.0}) z {:.1} m  slope {:.0} %  {}  water {:.1} m",
                    baked.elevation.height(x, y),
                    baked.elevation.node_slope(i, j) * 100.0,
                    lc.name,
                    baked.water_depth.get(i, j)
                );
            }
        }
        "bench" => bench(&data, &ds, &a)?,
        "duel" => duel(&ds, &a)?,
        "detect" => detect(&ds, &a)?,
        "firefight" => firefight(&ds, &a)?,
        "replay" => replay(&data, &ds, &a)?,
        _ => return Err(format!("unknown command '{cmd}'\n{USAGE}")),
    }
    Ok(())
}

/// Box-filter an RGBA image by an integer factor.
fn downsample(img: &[u8], w: usize, h: usize, n: usize) -> (Vec<u8>, usize, usize) {
    if n == 1 {
        return (img.to_vec(), w, h);
    }
    let (ow, oh) = (w / n, h / n);
    let mut out = vec![0u8; ow * oh * 4];
    for y in 0..oh {
        for x in 0..ow {
            for c in 0..4 {
                let mut sum = 0u32;
                for dy in 0..n {
                    for dx in 0..n {
                        sum += img[((y * n + dy) * w + x * n + dx) * 4 + c] as u32;
                    }
                }
                out[(y * ow + x) * 4 + c] = (sum / (n * n) as u32) as u8;
            }
        }
    }
    (out, ow, oh)
}

fn print_info(b: &BakedMap) {
    let m = &b.manifest;
    let (lo, hi) = b.elevation.min_max();
    println!(
        "map        {} ({} x {} cells @ {} m = {:.1} x {:.1} km)",
        m.id,
        m.width_cells,
        m.height_cells,
        m.cell_size_m,
        m.width_m() / 1000.0,
        m.height_m() / 1000.0
    );
    println!("elevation  {lo:.1} .. {hi:.1} m");
    println!(
        "features   {} rivers, {} roads, {} forests, {} buildings",
        b.features.rivers.len(),
        b.features.roads.len(),
        b.features.forests.len(),
        b.features.buildings.len()
    );
    let enterable = b.buildings.iter().filter(|x| !x.entrances.is_empty()).count();
    let entrances: usize = b.buildings.iter().map(|x| x.entrances.len()).sum();
    println!("buildings  {} baked, {enterable} enterable on foot, {entrances} entrances", b.buildings.len());
    println!(
        "contours   {} lines, {} points",
        b.contours.len(),
        b.contours.iter().map(|c| c.points.len()).sum::<usize>()
    );
    for l in &b.mobility {
        let nogo = l.speed.data.iter().filter(|&&v| v == 0).count() as f64 / l.speed.data.len() as f64;
        println!("mobility   {:<12} no-go {:.1} %", l.class.id, nogo * 100.0);
    }
    println!("hash       input {:016x} output {:016x}", b.input_hash, b.output_hash());
}

/// Re-simulate a saved replay (client CTRL+R) and check it reaches the recorded state hash.
fn replay(data: &std::path::Path, ds: &DataSet, a: &Args) -> Result<(), String> {
    use std::sync::Arc;

    let path = a.req("--file")?;
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let rep = sim::Replay::from_ron(&text)?;
    let sc = DataSet::load_scenario(data, &rep.scenario).map_err(|e| e.to_string())?;
    let map = Arc::new(workflow::ensure_map(data, ds, &sc.map, |s| eprintln!("{s}")).map_err(|e| e.to_string())?);
    let mut sim = sim::Sim::new(&sc, Arc::new(ds.clone()), map).map_err(|e| e.to_string())?;
    let t = Instant::now();
    rep.play(&mut sim, |s, ev| {
        for e in ev {
            use sim::PlatoonReport as R;
            let show = matches!(
                e,
                sim::Event::ScenarioEnd { .. } | sim::Event::TriggerFired { .. } | sim::Event::CodeWordSent { .. }
            ) || matches!(
                e,
                sim::Event::PlatoonReport {
                    report: R::AtCheckpoint { .. } | R::PlanBranch { .. } | R::PlanComplete | R::SetInPosition { .. },
                    ..
                }
            );
            if show {
                println!("t={:7.1}s  {e:?}", s.time_s());
            }
        }
    });
    let hash = sim.state_hash();
    println!(
        "replayed {} commands, {} ticks in {:.1} s; hash {hash:#018x} (recorded {:#018x})",
        rep.commands.len(),
        rep.final_tick,
        t.elapsed().as_secs_f64(),
        rep.final_hash
    );
    if hash != rep.final_hash {
        return Err("replay diverged from the recording".into());
    }
    Ok(())
}

fn run_scenario(data: &std::path::Path, ds: &DataSet, a: &Args) -> Result<(), String> {
    use std::sync::Arc;

    use sim::{Command, Event, SideId, Sim};

    let id = a.req("--scenario")?;
    let mut sc = DataSet::load_scenario(data, id).map_err(|e| e.to_string())?;
    if let Some(seed) = a.get("--seed") {
        sc.seed = seed.parse().map_err(|_| "bad --seed")?;
    }
    let summary = a.0.iter().any(|x| x == "--summary");
    let map = Arc::new(workflow::ensure_map(data, ds, &sc.map, |s| eprintln!("{s}")).map_err(|e| e.to_string())?);
    let mut sim = Sim::new(&sc, Arc::new(ds.clone()), map).map_err(|e| e.to_string())?;
    // Default: play past the scenario's time limit so its end condition fires (a cut-off at the
    // limit itself stops one tick early and reads as a TimeOut); 300 s without a limit.
    let seconds: f64 = match a.get("--seconds") {
        Some(s) => s.parse().map_err(|_| "bad --seconds")?,
        None => sc.victory.as_ref().map_or(300.0, |v| v.time_limit_s + 60.0),
    };
    let blue = SideId(0);
    if let Some(m) = a.get("--move") {
        let (x, y) = m.split_once(',').ok_or("--move expects x,y")?;
        let dest = [x.parse::<f64>().map_err(|_| "bad x")?, y.parse::<f64>().map_err(|_| "bad y")?];
        let unit = sim.perceived(blue).own.first().ok_or("no Blue units")?.id;
        sim.submit(blue, Command::Move { unit, destination: dest, via: vec![], speed_limit_mps: None });
    }
    let t = Instant::now();
    let ticks = (seconds * sim::TICK_HZ as f64) as u64;
    let mut end = None;
    // Men lost by cause (small arms, HE, with the carrier): calibration notes §1.3 V4.
    let mut men_lost = [0u32; 3];
    for _ in 0..ticks {
        if sim.ended() {
            break;
        }
        for e in sim.step() {
            if let Event::ScenarioEnd { winner, reason } = &e {
                end = Some((*winner, format!("{reason:?}")));
            }
            if let Event::Casualties { lost, cause, .. } = &e {
                men_lost[*cause as usize] += *lost as u32;
            }
            let show = !summary
                && matches!(
                    e,
                    Event::ScenarioEnd { .. } | Event::PlatoonReport { .. } | Event::Hit { kill: Some(_), .. }
                );
            if show {
                println!("t={:7.1}s  {e:?}", sim.time_s());
            }
        }
    }
    let wall = t.elapsed().as_secs_f64();
    if summary {
        // scenario,seed,winner,end_s,reason,losses per side (destroyed or without firepower)
        let w = sim.truth();
        let lost = |side: u8| {
            (0..w.len())
                .filter(|&k| {
                    w.side[k].0 == side
                        && matches!(
                            w.condition[k],
                            sim::view::Condition::Destroyed | sim::view::Condition::FirepowerKill
                        )
                })
                .count()
        };
        let total = |side: u8| (0..w.len()).filter(|&k| w.side[k].0 == side).count();
        let (winner, reason) = end.unwrap_or((None, "TimeOut".into()));
        let name = |wn: Option<SideId>| wn.map_or("none".to_owned(), |s| sim.side_names()[s.0 as usize].clone());
        println!(
            "{id},{},{},{:.0},{},{}/{},{}/{}",
            sc.seed,
            name(winner),
            sim.time_s(),
            reason.split_whitespace().next().unwrap_or(""),
            lost(0),
            total(0),
            lost(1),
            total(1)
        );
        return Ok(());
    }
    for u in &sim.perceived(blue).own {
        println!(
            "{:<4} pos ({:7.1}, {:7.1})  speed {:4.1} m/s  fuel {:5.1} %",
            u.callsign,
            u.pos[0],
            u.pos[1],
            u.speed,
            u.fuel_fraction * 100.0
        );
    }
    let total: u32 = men_lost.iter().sum();
    if total > 0 {
        let pc = |n: u32| 100.0 * n as f64 / total as f64;
        println!(
            "men lost {total}: small arms {:.0} %  HE {:.0} %  with the carrier {:.0} %",
            pc(men_lost[0]),
            pc(men_lost[1]),
            pc(men_lost[2])
        );
    }
    println!("ticks {ticks}  wall {wall:.2} s  ({:.0}x real time)", seconds / wall.max(1e-9));
    println!("state hash {:016x}", sim.state_hash());
    Ok(())
}

/// Duel validation harness (CLAUDE.md testing): stationary shooter vs. stationary target on
/// flat open ground by day; the target holds fire but still screens itself with smoke when shot
/// at (M7a drill), so a hit that does not kill delays the next shot by the screen's life (about a
/// minute; `kill_within_120s` counts this, N10). Writes one CSV row per range.
fn duel(ds: &DataSet, a: &Args) -> Result<(), String> {
    use std::fmt::Write as _;
    use std::sync::Arc;

    use sim::view::{Condition, WeaponsStatus};
    use sim::{Addressee, Command, EntityId, Event, SideId, Sim};
    use sim_data::{MapRef, ScenarioDef, SideDef, UnitDef};
    use terrain::mobility::MobilityClass;
    use terrain::{AuthoringPackage, MapManifest};

    let shooter = a.get("--shooter").unwrap_or("bulwark_a1").to_owned();
    let target = a.get("--target").unwrap_or("kurgan_b").to_owned();
    let trials: u64 = a.get("--trials").unwrap_or("200").parse().map_err(|_| "bad --trials")?;
    let ranges: Vec<f64> = a
        .get("--ranges")
        .unwrap_or("500,1000,1500,2000,2500,3000,3500")
        .split(',')
        .map(|r| r.trim().parse::<f64>().map_err(|_| format!("bad range '{r}'")))
        .collect::<Result<_, _>>()?;
    let side_on = a.get("--aspect") == Some("side");
    // A squad shooter (plan M8B-AT-TEAMS §10.5): its launchers, set up, ambush sprung.
    let squad_shooter = ds.squad(&shooter).is_some();
    let year: i32 = a.get("--year").unwrap_or("1990").parse().map_err(|_| "bad --year")?;
    let max_r = ranges.iter().copied().fold(0.0, f64::max);
    let h_cells = ((max_r + 800.0) / 4.0) as usize;
    let m = MapManifest {
        id: "duel".into(),
        name: "duel".into(),
        width_cells: 200,
        height_cells: h_cells,
        cell_size_m: 4.0,
        seed: 1,
        recipe: None,
    };
    let mut pkg = AuthoringPackage::new_flat(m, 100.0);
    pkg.landcover.data.fill(ds.terrain.landcover_id("road").ok_or("no road class")?); // no micro-relief
    let classes: Vec<MobilityClass> = terrain::mobility::all_classes(ds);
    let map = Arc::new(terrain::bake(&pkg, &ds.terrain, &classes));
    let data = Arc::new(ds.clone());

    let mut csv = String::from(
        "range_m,trials,mean_detect_s,mean_first_shot_s,ph_first_round,ph_all_rounds,p_pen_given_hit,kill_within_120s,mean_time_to_kill_s,mean_rounds_to_kill\n",
    );
    for &r in &ranges {
        let (mut det, mut det_n, mut first, mut first_n) = (0.0, 0, 0.0, 0);
        let (mut fr_hit, mut fr_n, mut hits, mut shots, mut pens) = (0, 0, 0, 0, 0);
        let (mut kills, mut ttk, mut rtk) = (0, 0.0, 0.0);
        for trial in 0..trials {
            let sc = ScenarioDef {
                id: "duel".into(),
                name: "duel".into(),
                date: (year, 6, 1),
                start_time_s: 10 * 3600,
                map: MapRef { id: "duel".into(), recipe: String::new(), seed: 1, size_m: 0.0 },
                seed: trial + 1,
                sides: vec![
                    SideDef {
                        name: "A".into(),
                        doctrine: "western-combined".into(),
                        tech_year: None,
                        units: vec![UnitDef {
                            callsign: "S".into(),
                            vehicle: if squad_shooter { String::new() } else { shooter.clone() },
                            position: [400.0, 200.0],
                            heading_deg: 0.0,
                            platoon: None,
                            squad: squad_shooter.then(|| shooter.clone()),
                            mounted_in: None,
                            cargo: None,
                        }],
                        ai: None,
                        fire_support: vec![],
                        supply_points: vec![],
                    },
                    SideDef {
                        name: "B".into(),
                        doctrine: "generic-opfor".into(),
                        tech_year: None,
                        units: vec![UnitDef {
                            callsign: "T".into(),
                            vehicle: target.clone(),
                            position: [400.0, 200.0 + r],
                            heading_deg: if side_on { 90.0 } else { 180.0 },
                            platoon: None,
                            squad: None,
                            mounted_in: None,
                            cargo: None,
                        }],
                        ai: None,
                        fire_support: vec![],
                        supply_points: vec![],
                    },
                ],
                objectives: vec![],
                victory: None,
                weather: Default::default(),
                briefing: None,
                logistics: vec![],
            };
            let mut sim = Sim::new(&sc, data.clone(), map.clone())?;
            sim.submit_immediate(
                SideId(1),
                Command::WeaponsControl { to: Addressee::Unit(EntityId(2)), status: WeaponsStatus::Hold },
            );
            if let Some(s) = sim.truth_mut().squad[0].as_mut() {
                s.sprung = true;
                s.halted_s = 1000.0;
                for w in s.weapons.iter_mut() {
                    w.bursts = 0; // launchers only
                }
            }
            let mut seen = false;
            let mut rounds = 0;
            let mut results = 0;
            for _ in 0..(120 * sim::TICK_HZ) {
                for e in sim.step() {
                    match e {
                        Event::Fired { unit: EntityId(1), .. } => {
                            if rounds == 0 {
                                first += sim.time_s();
                                first_n += 1;
                            }
                            rounds += 1;
                        }
                        Event::ShotResult { unit: EntityId(1), hit, .. } => {
                            shots += 1;
                            results += 1;
                            if hit {
                                hits += 1;
                            }
                            if results == 1 {
                                fr_n += 1;
                                if hit {
                                    fr_hit += 1;
                                }
                            }
                        }
                        Event::Hit { unit: EntityId(2), penetrated, .. } if penetrated => pens += 1,
                        _ => {}
                    }
                }
                if !seen && !sim.perceived(SideId(0)).contacts.is_empty() {
                    seen = true;
                    det += sim.time_s();
                    det_n += 1;
                }
                if matches!(sim.truth().condition[1], Condition::Destroyed | Condition::FirepowerKill) {
                    kills += 1;
                    ttk += sim.time_s();
                    rtk += rounds as f64;
                    break;
                }
            }
        }
        let div = |a: f64, b: usize| if b > 0 { a / b as f64 } else { f64::NAN };
        let _ = writeln!(
            csv,
            "{r:.0},{trials},{:.1},{:.1},{:.3},{:.3},{:.3},{:.3},{:.1},{:.2}",
            div(det, det_n),
            div(first, first_n),
            div(fr_hit as f64, fr_n),
            div(hits as f64, shots),
            div(pens as f64, hits),
            kills as f64 / trials as f64,
            div(ttk, kills),
            div(rtk, kills)
        );
    }
    match a.get("--out") {
        Some(p) => {
            if let Some(dir) = std::path::Path::new(p).parent() {
                std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            }
            std::fs::write(p, &csv).map_err(|e| format!("{p}: {e}"))?;
            eprintln!("wrote {p}");
            print!("{csv}");
        }
        None => print!("{csv}"),
    }
    Ok(())
}

/// Infantry time-to-detect (plan M8B-INFANTRY §3 deliverable): a vehicle observer and one squad at
/// each range and posture on flat ground; per trial the time of the crew's first detection and
/// recognition (truth tracks), up to 120 s.
fn detect(ds: &DataSet, a: &Args) -> Result<(), String> {
    use std::fmt::Write as _;
    use std::sync::Arc;

    use sim::view::WeaponsStatus;
    use sim::{Addressee, Command, EntityId, SideId, Sim};
    use sim_data::{MapRef, ScenarioDef, SideDef, UnitDef};
    use terrain::{AuthoringPackage, MapManifest};

    let observer = a.get("--observer").unwrap_or("bulwark_a1").to_owned();
    let squad = a.get("--squad").unwrap_or("mr_squad").to_owned();
    let trials: u64 = a.get("--trials").unwrap_or("100").parse().map_err(|_| "bad --trials")?;
    let year: i32 = a.get("--year").unwrap_or("1990").parse().map_err(|_| "bad --year")?;
    let hour: f64 = a.get("--hour").unwrap_or("10").parse().map_err(|_| "bad --hour")?;
    let cover = a.get("--cover").unwrap_or("grassland");
    let ranges: Vec<f64> = a
        .get("--ranges")
        .unwrap_or("250,500,750,1000,1500,2000")
        .split(',')
        .map(|r| r.trim().parse::<f64>().map_err(|_| format!("bad range '{r}'")))
        .collect::<Result<_, _>>()?;
    let postures: Vec<String> =
        a.get("--posture").unwrap_or("moving,halted,dug_in").split(',').map(|p| p.trim().to_owned()).collect();
    let max_r = ranges.iter().copied().fold(0.0, f64::max);
    let m = MapManifest {
        id: "detect".into(),
        name: "detect".into(),
        width_cells: 400,
        height_cells: ((max_r + 800.0) / 4.0) as usize,
        cell_size_m: 4.0,
        seed: 1,
        recipe: None,
    };
    let mut pkg = AuthoringPackage::new_flat(m, 100.0);
    pkg.landcover.data.fill(ds.terrain.landcover_id(cover).ok_or_else(|| format!("no land cover '{cover}'"))?);
    let map = Arc::new(terrain::bake(&pkg, &ds.terrain, &terrain::mobility::all_classes(ds)));
    let data = Arc::new(ds.clone());
    let unit = |cs: &str, vehicle: &str, squad: Option<&str>, pos: [f64; 2], heading_deg: f64| UnitDef {
        callsign: cs.into(),
        vehicle: vehicle.into(),
        position: pos,
        heading_deg,
        platoon: None,
        squad: squad.map(str::to_owned),
        mounted_in: None,
        cargo: None,
    };
    let side = |name: &str, doctrine: &str, u: UnitDef| SideDef {
        name: name.into(),
        doctrine: doctrine.into(),
        tech_year: None,
        units: vec![u],
        ai: None,
        fire_support: vec![],
        supply_points: vec![],
    };
    // The observer is a vehicle, or a squad (its eyes, binoculars and night devices).
    let observer_squad = ds.squad(&observer).is_some();
    let observer_unit = |x: f64| match observer_squad {
        true => unit("O", "", Some(&observer), [x, 200.0], 0.0),
        false => unit("O", &observer, None, [x, 200.0], 0.0),
    };
    let mut csv =
        String::from("posture,range_m,trials,p_detect_120s,mean_detect_s,p_recognise_120s,mean_recognise_s\n");
    for posture in &postures {
        for &r in &ranges {
            let (mut det, mut det_t, mut rec, mut rec_t) = (0u64, 0.0, 0u64, 0.0);
            for trial in 0..trials {
                // A new column of ground each trial, so micro-relief is averaged over.
                let x = 300.0 + (trial % 100) as f64 * 9.7;
                let sc = ScenarioDef {
                    id: "detect".into(),
                    name: "detect".into(),
                    date: (year, 6, 1),
                    start_time_s: (hour * 3600.0) as u32,
                    map: MapRef { id: "detect".into(), recipe: String::new(), seed: 1, size_m: 0.0 },
                    seed: trial + 1,
                    sides: vec![
                        side("A", "western-combined", observer_unit(x)),
                        side("B", "generic-opfor", unit("T", "", Some(&squad), [x, 200.0 + r], 180.0)),
                    ],
                    objectives: vec![],
                    victory: None,
                    weather: Default::default(),
                    briefing: None,
                    logistics: vec![],
                };
                let mut sim = Sim::new(&sc, data.clone(), map.clone())?;
                for (s, id) in [(SideId(0), EntityId(1)), (SideId(1), EntityId(2))] {
                    sim.submit_immediate(
                        s,
                        Command::WeaponsControl { to: Addressee::Unit(id), status: WeaponsStatus::Hold },
                    );
                }
                match posture.as_str() {
                    // Walking across the observer's line of sight.
                    "moving" => sim.submit_immediate(
                        SideId(1),
                        Command::Move {
                            unit: EntityId(2),
                            destination: [x + 300.0, 200.0 + r],
                            via: vec![],
                            speed_limit_mps: None,
                        },
                    ),
                    "halted" => {}
                    "dug_in" => {
                        if let Some(s) = sim.truth_mut().squad[1].as_mut() {
                            s.halted_s = 1.0e6;
                        }
                    }
                    other => return Err(format!("unknown posture '{other}'")),
                }
                let (mut d, mut c) = (None, None);
                for _ in 0..(120 * sim::TICK_HZ) {
                    sim.step();
                    if let Some(tr) = sim.truth().sensors[0].as_ref().and_then(|s| s.track(1)) {
                        d.get_or_insert(sim.time_s());
                        if tr.level >= sim::view::ContactLevel::Recognised {
                            c.get_or_insert(sim.time_s());
                        }
                    }
                    if c.is_some() {
                        break;
                    }
                }
                if let Some(t) = d {
                    det += 1;
                    det_t += t;
                }
                if let Some(t) = c {
                    rec += 1;
                    rec_t += t;
                }
            }
            let div = |a: f64, b: u64| if b > 0 { a / b as f64 } else { f64::NAN };
            let _ = writeln!(
                csv,
                "{posture},{r:.0},{trials},{:.3},{:.1},{:.3},{:.1}",
                det as f64 / trials as f64,
                div(det_t, det),
                rec as f64 / trials as f64,
                div(rec_t, rec)
            );
        }
    }
    match a.get("--out") {
        Some(p) => {
            std::fs::write(p, &csv).map_err(|e| format!("{p}: {e}"))?;
            eprintln!("wrote {p}");
            print!("{csv}");
        }
        None => print!("{csv}"),
    }
    Ok(())
}

/// Small-arms validation harness (docs/plans/M8B-CALIBRATION-NOTES.md §1.3, V2–V3): one squad
/// fires at another on flat ground; the target holds fire and stays in its posture. Per range and
/// posture: rounds fired, casualties, rounds per casualty and casualties per weapon-minute from the
/// first volley, for the shooter's weapons whose names contain one of `--weapons` (others silent).
/// `--mutual` lets the target fire back with all its weapons (V5: exchange ratio by year).
fn firefight(ds: &DataSet, a: &Args) -> Result<(), String> {
    use std::fmt::Write as _;
    use std::sync::Arc;

    use sim::view::WeaponsStatus;
    use sim::{Addressee, CasualtyCause, Command, EntityId, Event, SideId, Sim};
    use sim_data::{MapRef, ScenarioDef, SideDef, UnitDef};
    use terrain::{AuthoringPackage, MapManifest};

    let shooter = a.get("--shooter").unwrap_or("rifle_squad_b").to_owned();
    let target = a.get("--target").unwrap_or("mr_squad").to_owned();
    let trials: u64 = a.get("--trials").unwrap_or("100").parse().map_err(|_| "bad --trials")?;
    let seconds: f64 = a.get("--seconds").unwrap_or("120").parse().map_err(|_| "bad --seconds")?;
    let year: i32 = a.get("--year").unwrap_or("1990").parse().map_err(|_| "bad --year")?;
    let hour: f64 = a.get("--hour").unwrap_or("10").parse().map_err(|_| "bad --hour")?;
    let cover = a.get("--cover").unwrap_or("grassland");
    let ranges: Vec<f64> = a
        .get("--ranges")
        .unwrap_or("100,250,400")
        .split(',')
        .map(|r| r.trim().parse::<f64>().map_err(|_| format!("bad range '{r}'")))
        .collect::<Result<_, _>>()?;
    let postures: Vec<String> =
        a.get("--posture").unwrap_or("halted").split(',').map(|p| p.trim().to_owned()).collect();
    let picks: Vec<String> = a.get("--weapons").unwrap_or("all").split(',').map(|w| w.trim().to_uppercase()).collect();
    let mutual = a.0.iter().any(|x| x == "--mutual");
    let sdef = ds.squad(&shooter).ok_or_else(|| format!("no squad '{shooter}'"))?;
    let kept: Vec<usize> = (0..sdef.weapons.len())
        .filter(|&i| {
            let w = &sdef.weapons[i];
            w.in_service.is_none_or(|d| d.contains(year))
                && picks.iter().any(|p| p == "ALL" || w.name.contains(p.as_str()))
        })
        .collect();
    if kept.is_empty() {
        return Err(format!("no weapon of '{shooter}' in service in {year} matches --weapons"));
    }
    let guns: u32 = kept.iter().map(|&i| sdef.weapons[i].count as u32).sum();
    let label = kept.iter().map(|&i| sdef.weapons[i].name.as_str()).collect::<Vec<_>>().join("+");
    let max_r = ranges.iter().copied().fold(0.0, f64::max);
    let m = MapManifest {
        id: "firefight".into(),
        name: "firefight".into(),
        width_cells: 400,
        height_cells: ((max_r + 800.0) / 4.0) as usize,
        cell_size_m: 4.0,
        seed: 1,
        recipe: None,
    };
    let mut pkg = AuthoringPackage::new_flat(m, 100.0);
    pkg.landcover.data.fill(ds.terrain.landcover_id(cover).ok_or_else(|| format!("no land cover '{cover}'"))?);
    let map = Arc::new(terrain::bake(&pkg, &ds.terrain, &terrain::mobility::all_classes(ds)));
    let data = Arc::new(ds.clone());
    let unit = |cs: &str, squad: &str, pos: [f64; 2], heading_deg: f64| UnitDef {
        callsign: cs.into(),
        vehicle: String::new(),
        position: pos,
        heading_deg,
        platoon: None,
        squad: Some(squad.to_owned()),
        mounted_in: None,
        cargo: None,
    };
    let side = |name: &str, doctrine: &str, u: UnitDef| SideDef {
        name: name.into(),
        doctrine: doctrine.into(),
        tech_year: None,
        units: vec![u],
        ai: None,
        fire_support: vec![],
        supply_points: vec![],
    };
    let mut csv = String::from(
        "posture,range_m,weapons,guns,trials,p_engaged,mean_first_volley_s,rounds,casualties,rounds_per_casualty,casualties_per_gun_min,shooter_casualties,exchange_ratio\n",
    );
    for posture in &postures {
        for &r in &ranges {
            let (mut engaged, mut first_sum, mut rounds, mut lost, mut gun_min) = (0u64, 0.0, 0.0, 0u64, 0.0);
            let mut own_lost = 0u64;
            for trial in 0..trials {
                let x = 300.0 + (trial % 100) as f64 * 9.7;
                let sc = ScenarioDef {
                    id: "firefight".into(),
                    name: "firefight".into(),
                    date: (year, 6, 1),
                    start_time_s: (hour * 3600.0) as u32,
                    map: MapRef { id: "firefight".into(), recipe: String::new(), seed: 1, size_m: 0.0 },
                    seed: trial + 1,
                    sides: vec![
                        side("A", "western-combined", unit("S", &shooter, [x, 200.0], 0.0)),
                        side("B", "generic-opfor", unit("T", &target, [x, 200.0 + r], 180.0)),
                    ],
                    objectives: vec![],
                    victory: None,
                    weather: Default::default(),
                    briefing: None,
                    logistics: vec![],
                };
                let mut sim = Sim::new(&sc, data.clone(), map.clone())?;
                if !mutual {
                    sim.submit_immediate(
                        SideId(1),
                        Command::WeaponsControl { to: Addressee::Unit(EntityId(2)), status: WeaponsStatus::Hold },
                    );
                }
                if let Some(s) = sim.truth_mut().squad[0].as_mut() {
                    for w in s.weapons.iter_mut().filter(|w| !kept.contains(&w.def)) {
                        w.bursts = 0;
                    }
                }
                match posture.as_str() {
                    "moving" => sim.submit_immediate(
                        SideId(1),
                        Command::Move {
                            unit: EntityId(2),
                            destination: [x + 300.0, 200.0 + r],
                            via: vec![],
                            speed_limit_mps: None,
                        },
                    ),
                    "halted" => {}
                    "dug_in" => {
                        if let Some(s) = sim.truth_mut().squad[1].as_mut() {
                            s.halted_s = 1.0e6;
                        }
                    }
                    other => return Err(format!("unknown posture '{other}'")),
                }
                let mut first = None;
                for _ in 0..((seconds * sim::TICK_HZ as f64) as u64) {
                    for e in sim.step() {
                        match e {
                            Event::Volley { unit: EntityId(1), .. } => {
                                first.get_or_insert(sim.time_s());
                            }
                            Event::Casualties {
                                unit: EntityId(2), lost: n, cause: CasualtyCause::SmallArms, ..
                            } => {
                                lost += n as u64;
                            }
                            Event::Casualties {
                                unit: EntityId(1), lost: n, cause: CasualtyCause::SmallArms, ..
                            } => {
                                own_lost += n as u64;
                            }
                            _ => {}
                        }
                    }
                    let w = sim.truth();
                    if (0..2).any(|k| w.squad[k].as_ref().is_none_or(|s| s.strength == 0)) {
                        break;
                    }
                }
                if let Some(t0) = first {
                    engaged += 1;
                    first_sum += t0;
                    gun_min += guns as f64 * (sim.time_s() - t0) / 60.0;
                }
                if let Some(s) = sim.truth().squad[0].as_ref() {
                    rounds += s
                        .weapons
                        .iter()
                        .filter(|w| kept.contains(&w.def))
                        .map(|w| (w.full - w.bursts) as f64 * sdef.weapons[w.def].rounds_per_burst.get())
                        .sum::<f64>();
                }
            }
            let div = |a: f64, b: f64| if b > 0.0 { a / b } else { f64::NAN };
            let _ = writeln!(
                csv,
                "{posture},{r:.0},{label},{guns},{trials},{:.3},{:.1},{rounds:.0},{lost},{:.0},{:.3},{own_lost},{:.2}",
                engaged as f64 / trials as f64,
                div(first_sum, engaged as f64),
                div(rounds, lost as f64),
                div(lost as f64, gun_min),
                div(lost as f64, own_lost as f64)
            );
        }
    }
    match a.get("--out") {
        Some(p) => {
            std::fs::write(p, &csv).map_err(|e| format!("{p}: {e}"))?;
            eprintln!("wrote {p}");
            print!("{csv}");
        }
        None => print!("{csv}"),
    }
    Ok(())
}

/// One result table: printed aligned for reading, or as CSV (`--csv`, one block per table).
struct Table {
    title: &'static str,
    header: Vec<&'static str>,
    rows: Vec<Vec<String>>,
}

fn print_tables(tables: &[Table], csv: bool) {
    for t in tables {
        if csv {
            println!("{}", t.header.join(","));
            for r in &t.rows {
                println!("{}", r.join(","));
            }
            println!();
            continue;
        }
        println!("{}", t.title);
        let widths: Vec<usize> = (0..t.header.len())
            .map(|c| t.rows.iter().map(|r| r[c].len()).chain([t.header[c].len()]).max().unwrap_or(0))
            .collect();
        let line = |cells: Vec<&str>| {
            let parts: Vec<String> = cells.iter().zip(&widths).map(|(c, w)| format!("{c:>w$}", w = *w)).collect();
            println!("  {}", parts.join("  "));
        };
        line(t.header.clone());
        for r in &t.rows {
            line(r.iter().map(String::as_str).collect());
        }
        println!();
    }
}

/// Mean and 95th percentile (nearest rank) of a non-empty sample set.
fn mean_p95(mut v: Vec<f64>) -> (f64, f64) {
    v.sort_by(f64::total_cmp);
    let n = v.len();
    let mean = v.iter().sum::<f64>() / n as f64;
    let rank = ((n as f64 * 0.95).ceil() as usize).clamp(1, n);
    (mean, v[rank - 1])
}

/// Performance benchmarks (Z5): single LOS rays, viewshed, battle-position search and whole-sim
/// throughput, on the baked maps the scenarios use. Wall-clock timing is allowed here; sim logic
/// never reads the clock. Random inputs come from fixed-seed ChaCha streams.
fn bench(data: &std::path::Path, ds: &DataSet, a: &Args) -> Result<(), String> {
    use std::hint::black_box;
    use std::sync::Arc;

    use rand_chacha::ChaCha8Rng;
    use rand_chacha::rand_core::{Rng, SeedableRng};
    use sim::{Sim, TICK_HZ};
    use terrain::{ground::Ground, los};

    let csv = a.0.iter().any(|x| x == "--csv");
    let samples: usize = a.get("--samples").unwrap_or("4000").parse().map_err(|_| "bad --samples")?;
    let ticks: u64 = a.get("--ticks").unwrap_or("3000").parse().map_err(|_| "bad --ticks")?;
    if samples == 0 || ticks == 0 {
        return Err("--samples and --ticks must be > 0".into());
    }
    let profile = if cfg!(debug_assertions) { "DEBUG (use --release)" } else { "release" };
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    eprintln!("bench: build {profile}, {threads} hardware threads");

    // Eye height of the observer (the sim uses the observer's sight height) and the target's
    // five sample heights, exactly as sensors.rs builds them.
    let eye = ds.vehicle("bulwark_a0").ok_or("no vehicle bulwark_a0")?.dimensions.sight_height_m.get();
    let target = ds.vehicle("kurgan_b").ok_or("no vehicle kurgan_b")?;
    let target_heights = sim::sensors::sample_heights(target);

    // Maps are taken from the scenarios that use them, so the bench loads exactly what the game
    // loads (generated on first use, like every other subcommand).
    let mut maps = Vec::new();
    for sc_id in ["m5-hold-the-village", "cw-village-defence", "m8h-town"] {
        let sc = DataSet::load_scenario(data, sc_id).map_err(|e| e.to_string())?;
        let map = workflow::ensure_map(data, ds, &sc.map, |s| eprintln!("{s}")).map_err(|e| e.to_string())?;
        maps.push((sc, Arc::new(map)));
    }

    // Position-search inputs, as platoon_ai::assign_positions builds them: the objective is the
    // centre, the client's default radius is 250 m, the threat comes from the north-east.
    const BP_RADIUS: f64 = 250.0;
    const BP_FACING: f64 = std::f64::consts::FRAC_PI_4;
    const VIEWSHED_RADIUS: f64 = 3000.0;
    const VIEWSHED_STEP: f64 = 8.0;

    let mut los_rows = Vec::new();
    let mut vs_rows = Vec::new();
    let mut bp_rows = Vec::new();
    let mut sim_rows = Vec::new();
    let shared = Arc::new(ds.clone());

    for (sc, map) in &maps {
        let g = Ground::new(map, &ds.terrain);
        let name = map.manifest.id.clone();
        let (w, h) = (map.manifest.width_m(), map.manifest.height_m());
        let margin = 50.0;
        let inside = |p: [f64; 2]| p[0] >= margin && p[1] >= margin && p[0] <= w - margin && p[1] <= h - margin;
        let objective = sc.objectives.first().map_or([w / 2.0, h / 2.0], |o| o.centre);

        // a) Single LOS ray: random observer, random bearing, fixed range; each call timed alone.
        let mut rng = ChaCha8Rng::seed_from_u64(0x2_5a7);
        let unit = |rng: &mut ChaCha8Rng| (rng.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64);
        for range in [500.0, 1000.0, 2000.0, 4000.0] {
            let mut times = Vec::with_capacity(samples);
            let mut visible = 0usize;
            for _ in 0..samples {
                let (from, to) = loop {
                    let from =
                        [margin + unit(&mut rng) * (w - 2.0 * margin), margin + unit(&mut rng) * (h - 2.0 * margin)];
                    let bearing = unit(&mut rng) * std::f64::consts::TAU;
                    let to = [from[0] + range * bearing.sin(), from[1] + range * bearing.cos()];
                    if inside(to) {
                        break (from, to);
                    }
                };
                let t = Instant::now();
                let s = los::sight(black_box(&g), black_box(from), eye, black_box(to), &target_heights);
                times.push(t.elapsed().as_secs_f64() * 1e6);
                visible += usize::from(s.any());
            }
            let (mean, p95) = mean_p95(times);
            los_rows.push(vec![
                name.clone(),
                format!("{range:.0}"),
                samples.to_string(),
                format!("{mean:.1}"),
                format!("{p95:.1}"),
                format!("{:.3}", visible as f64 / samples as f64),
            ]);
        }

        // b) Viewshed as the client's viewshed tool calls it: 3 km radius, 8 m cells, 2.4 m
        // target height. Origins: the scenario objective plus three seeded interior points.
        let mut origins = vec![objective];
        for _ in 0..3 {
            origins.push([margin + unit(&mut rng) * (w - 2.0 * margin), margin + unit(&mut rng) * (h - 2.0 * margin)]);
        }
        for o in origins {
            let mut times = Vec::new();
            for _ in 0..5 {
                let t = Instant::now();
                let vs = los::viewshed(black_box(&g), black_box(o), eye, 2.4, VIEWSHED_RADIUS, VIEWSHED_STEP);
                times.push(t.elapsed().as_secs_f64() * 1e3);
                black_box(vs);
            }
            let (mean, p95) = mean_p95(times);
            vs_rows.push(vec![
                name.clone(),
                format!("{:.0},{:.0}", o[0], o[1]),
                format!("{VIEWSHED_RADIUS:.0}"),
                format!("{VIEWSHED_STEP:.0}"),
                format!("{mean:.1}"),
                format!("{p95:.1}"),
            ]);
        }

        // c) Battle-position search: the first side's first vehicle type, its first four units as
        // the movers, its doctrine dispersion. Same passability test as platoon_ai.rs.
        let side = &sc.sides[0];
        let vehicle_id = &side.units[0].vehicle;
        let v = ds.vehicle(vehicle_id).ok_or("no BP vehicle")?;
        let dispersion = ds.doctrine.iter().find(|d| d.id == side.doctrine).ok_or("no doctrine")?.bp_dispersion_m.get();
        let q = ai::position::BpQuery {
            centre: objective,
            radius: BP_RADIUS,
            facing: BP_FACING,
            hull_height: v.dimensions.hull_height_m.get(),
            sight_height: v.dimensions.sight_height_m.get(),
            gun_depression: -v.gun.depression_rad.get(),
            turret_down: false,
        };
        let layer = map.mobility(vehicle_id).ok_or("no mobility layer for BP vehicle")?;
        let hf = &map.elevation;
        let passable = |x: f64, y: f64| {
            let (i, j) = hf.cell_of(x, y);
            layer.speed.get(i, j) > 0 && hf.node_slope(i, j) < 0.3
        };
        let starts: Vec<[f64; 2]> = side.units.iter().take(4).map(|u| u.position).collect();
        let (mut eval_ms, mut assign_ms, mut total_ms) = (Vec::new(), Vec::new(), Vec::new());
        let (mut n_cands, mut n_chosen) = (0, 0);
        for _ in 0..10 {
            let t0 = Instant::now();
            let cands = ai::position::evaluate(&g, black_box(&q), &passable);
            let t1 = Instant::now();
            let chosen = ai::position::assign(&cands, starts.len(), dispersion, &starts);
            let t2 = Instant::now();
            n_cands = cands.len();
            n_chosen = chosen.len();
            eval_ms.push((t1 - t0).as_secs_f64() * 1e3);
            assign_ms.push((t2 - t1).as_secs_f64() * 1e3);
            total_ms.push((t2 - t0).as_secs_f64() * 1e3);
            black_box(chosen);
        }
        let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
        let (tot_mean, tot_p95) = mean_p95(total_ms);
        bp_rows.push(vec![
            name.clone(),
            sc.id.clone(),
            format!("{vehicle_id} x {}", starts.len()),
            n_cands.to_string(),
            n_chosen.to_string(),
            format!("{:.2}", mean(&eval_ms)),
            format!("{:.3}", mean(&assign_ms)),
            format!("{tot_mean:.2}"),
            format!("{tot_p95:.2}"),
        ]);

        // d) Whole-sim throughput: step the scenario's sim with its own AI for `ticks` ticks.
        let t = Instant::now();
        let mut sim = Sim::new(sc, shared.clone(), Arc::clone(map))?;
        let setup_ms = t.elapsed().as_secs_f64() * 1e3;
        let w_state = sim.truth();
        let entities = w_state.len();
        let side_counts: Vec<String> = sim
            .side_names()
            .iter()
            .enumerate()
            .map(|(s, n)| format!("{n} {}", (0..entities).filter(|&k| w_state.side[k].0 == s as u8).count()))
            .collect();
        let mut run_ticks = 0u64;
        let t = Instant::now();
        for _ in 0..ticks {
            if sim.ended() {
                break;
            }
            black_box(sim.step());
            run_ticks += 1;
        }
        let wall = t.elapsed().as_secs_f64();
        let tps = run_ticks as f64 / wall.max(1e-9);
        sim_rows.push(vec![
            sc.id.clone(),
            format!("{entities} ({})", side_counts.join(", ")),
            run_ticks.to_string(),
            format!("{setup_ms:.0}"),
            format!("{wall:.2}"),
            format!("{tps:.0}"),
            format!("{:.1}", tps / TICK_HZ as f64),
        ]);
    }

    let tables = [
        Table {
            title: "LOS single ray (terrain::los::sight, 5 target heights, eye 2.3 m, bulwark_a0 -> kurgan_b)",
            header: vec!["map", "range_m", "pairs", "mean_us", "p95_us", "visible"],
            rows: los_rows,
        },
        Table {
            title: "Viewshed (terrain::los::viewshed, radius 3000 m, step 8 m, eye 2.3 m, target 2.4 m)",
            header: vec!["map", "origin_x,y", "radius_m", "step_m", "mean_ms", "p95_ms"],
            rows: vs_rows,
        },
        Table {
            title: "Battle-position search (ai::position evaluate + assign, radius 250 m, facing 45 deg, 10 calls)",
            header: vec![
                "map",
                "scenario",
                "vehicle x movers",
                "cands",
                "chosen",
                "eval_ms",
                "assign_ms",
                "mean_ms",
                "p95_ms",
            ],
            rows: bp_rows,
        },
        Table {
            title: "Sim throughput (Sim::step with scenario AI; 20 ticks/s = 1x real time)",
            header: vec!["scenario", "entities (per side)", "ticks", "setup_ms", "wall_s", "ticks_per_s", "x_1x_speed"],
            rows: sim_rows,
        },
    ];
    print_tables(&tables, csv);
    Ok(())
}
