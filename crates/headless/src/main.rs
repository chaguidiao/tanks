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
  run     --scenario <id> [--seconds <s>] [--move <x,y>]               run a scenario headless
  replay  --file <replay.ron>                                          re-simulate a replay, verify its hash
  duel    [--shooter <id>] [--target <id>] [--ranges 500,1000,..] [--trials <n>] [--aspect front|side]
          [--out <csv>]                                                Monte-Carlo duel validation harness

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
        "duel" => duel(&ds, &a)?,
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
            if matches!(e, sim::Event::ScenarioEnd { .. } | sim::Event::TriggerFired { .. }) {
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
    let sc = DataSet::load_scenario(data, id).map_err(|e| e.to_string())?;
    let map = Arc::new(workflow::ensure_map(data, ds, &sc.map, |s| eprintln!("{s}")).map_err(|e| e.to_string())?);
    let mut sim = Sim::new(&sc, Arc::new(ds.clone()), map).map_err(|e| e.to_string())?;
    let seconds: f64 = a.get("--seconds").unwrap_or("300").parse().map_err(|_| "bad --seconds")?;
    let blue = SideId(0);
    if let Some(m) = a.get("--move") {
        let (x, y) = m.split_once(',').ok_or("--move expects x,y")?;
        let dest = [x.parse::<f64>().map_err(|_| "bad x")?, y.parse::<f64>().map_err(|_| "bad y")?];
        let unit = sim.perceived(blue).own.first().ok_or("no Blue units")?.id;
        sim.submit(blue, Command::Move { unit, destination: dest, via: vec![], speed_limit_mps: None });
    }
    let t = Instant::now();
    let ticks = (seconds * sim::TICK_HZ as f64) as u64;
    for _ in 0..ticks {
        if sim.ended() {
            break;
        }
        for e in sim.step() {
            let show =
                matches!(e, Event::ScenarioEnd { .. } | Event::PlatoonReport { .. } | Event::Hit { kill: Some(_), .. });
            if show {
                println!("t={:7.1}s  {e:?}", sim.time_s());
            }
        }
    }
    let wall = t.elapsed().as_secs_f64();
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
    println!("ticks {ticks}  wall {wall:.2} s  ({:.0}x real time)", seconds / wall.max(1e-9));
    println!("state hash {:016x}", sim.state_hash());
    Ok(())
}

/// Duel validation harness (CLAUDE.md testing): stationary shooter vs. stationary target on
/// flat open ground by day; the target holds fire. Writes one CSV row per range.
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
    let classes: Vec<MobilityClass> = ds.vehicles.iter().map(MobilityClass::from).collect();
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
                date: (1990, 6, 1),
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
                            vehicle: shooter.clone(),
                            position: [400.0, 200.0],
                            heading_deg: 0.0,
                            platoon: None,
                        }],
                        ai: None,
                        fire_support: vec![],
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
                        }],
                        ai: None,
                        fire_support: vec![],
                    },
                ],
                objectives: vec![],
                victory: None,
                weather: Default::default(),
                briefing: None,
            };
            let mut sim = Sim::new(&sc, data.clone(), map.clone())?;
            sim.submit_immediate(
                SideId(1),
                Command::WeaponsControl { to: Addressee::Unit(EntityId(2)), status: WeaponsStatus::Hold },
            );
            let mut seen = false;
            let mut rounds = 0;
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
                            if hit {
                                hits += 1;
                            }
                            if rounds == 1 && shots > 0 {
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
            std::fs::write(p, &csv).map_err(|e| e.to_string())?;
            eprintln!("wrote {p}");
            print!("{csv}");
        }
        None => print!("{csv}"),
    }
    Ok(())
}
