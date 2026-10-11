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
  run     --scenario <id> [--seconds <s>] [--move <x,y>]               run a scenario headless

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
        "features   {} rivers, {} roads, {} forests",
        b.features.rivers.len(),
        b.features.roads.len(),
        b.features.forests.len()
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

fn run_scenario(_data: &std::path::Path, _ds: &DataSet, _a: &Args) -> Result<(), String> {
    Err("run: not implemented yet".into())
}
