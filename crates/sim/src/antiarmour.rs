//! Squad anti-armour fire (docs/plans/M8B-AT-TEAMS.md §2.2): rockets and missiles fired by
//! dismounted squads at vehicles, through the same shot resolution as a vehicle's gun.
//!
//! - Set-up: a launcher fires only from a halt, once the squad has been halted `setup_s`.
//! - Ambush (FM 100-2-1 (1984) ambush; ATP 3-21.8 (2016) ch. 7): hold fire until a recognised
//!   vehicle is inside the doctrine's trigger range or the squad is fired on; with
//!   `prefer_flank`, hold on a vehicle showing a front its rounds cannot defeat. Then prefer a
//!   target showing its side or rear, and the best penetration margin.
//! - Unguided rockets carry a range-estimation error through their drop, an aim error and a
//!   ballistic error; command-guided missiles only the tracking error and dispersion, and miss if
//!   the gunner is pinned or lost before impact (operator in the loop; ATP 3-20.15 App. F, react
//!   to an ATGM: suppress the gunner). Fire-and-forget missiles are immune.
//! - A launch is a strong firing cue (backblast) for every observer.

use ai::picture::{ContactLevel, WeaponsStatus};
use rand_chacha::ChaCha8Rng;
use sim_data::{AmbushDoctrine, AmmoDef, AntiArmourClass, DamageTables, PenetrationModel, SquadDef, VehicleDef};

use crate::gunnery::{self, Effect, Face, FireEvent, Projectile, ShotGeometry};
use crate::picture::gauss;
use crate::world::World;
use crate::{DT, dist};

/// Suppression above which a squad counts as fired on: its ambush is sprung.
const SPRUNG_SUPPRESSION: f64 = 0.1;

pub struct Ctx<'a> {
    pub world: &'a mut World,
    pub squads: &'a [SquadDef],
    pub vehicles: &'a [VehicleDef],
    pub ammo: &'a [AmmoDef],
    pub pen: &'a PenetrationModel,
    pub dmg: &'a DamageTables,
    pub map: &'a terrain::BakedMap,
    pub td: &'a sim_data::TerrainData,
    /// Per side: the doctrine's ambush rules.
    pub ambush: &'a [AmbushDoctrine],
    /// Suppression slows the next shot: intervals × (1 + slow_gain · s).
    pub slow_gain: f64,
    pub tick: u64,
    pub rng: &'a mut ChaCha8Rng,
}

/// One candidate shot: target, round (load entry), margin, flank shown, range.
struct Choice {
    target: usize,
    li: usize,
    margin: f64,
    flank: bool,
    range: f64,
}

/// Geometry of a shot from squad `k` (at its firing height) at vehicle `t`.
fn geometry(ctx: &Ctx, k: usize, t: usize, visible: f64) -> ShotGeometry {
    let w = &*ctx.world;
    let (sp, tt) = (w.transform[k], w.transform[t]);
    let r = dist(sp.pos, tt.pos);
    let br = libm::atan2(tt.pos[0] - sp.pos[0], tt.pos[1] - sp.pos[1]);
    let s = w.squad[k].as_ref().expect("squad");
    let (sig, floor) = crate::infantry::signature(&ctx.squads[s.def], s, ctx.map, ctx.td);
    let tdef = &ctx.vehicles[w.vehicle[t].expect("vehicle")];
    let tspeed = w.mobility[t].as_ref().map_or(0.0, |m| m.speed);
    ShotGeometry {
        range: r,
        visible,
        bearing_to_shooter: crate::wrap(br + std::f64::consts::PI),
        target_heading: tt.heading,
        crossing_speed: (tspeed * libm::sin(tt.heading - br)).abs(),
        shooter_moving: false,
        plunge: gunnery::plunge_angle(
            sp.z + floor + sig.eye_height_m.get(),
            tt.z,
            0.5 * tdef.dimensions.height_m.get(),
            r,
        ),
    }
}

/// Best shot for weapon `wi` of squad `k` among its fresh vehicle tracks, or None (ambush rules).
fn choose(ctx: &Ctx, k: usize, wi: usize, min_level: ContactLevel) -> Option<Choice> {
    let w = &*ctx.world;
    let s = w.squad[k].as_ref()?;
    let st = &s.anti_armour[wi];
    let def = &ctx.squads[s.def].anti_armour[st.def];
    let amb = &ctx.ambush[w.side[k].0 as usize];
    let trigger = match def.class {
        AntiArmourClass::Rocket => amb.rocket_trigger_m.get(),
        AntiArmourClass::Missile => amb.missile_trigger_m.get(),
    };
    let top_above = ctx.dmg.top_hit_above_deg.get().to_radians();
    let tracks = &w.sensors[k].as_ref()?.tracks;
    let mut best: Option<Choice> = None;
    for tr in tracks {
        let t = tr.target;
        let Some(tvi) = w.vehicle[t] else { continue };
        if tr.level < min_level
            || ctx.tick.saturating_sub(tr.last_seen_tick) > 20
            || tr.visible_fraction <= 0.0
            || !w.alive(t)
        {
            continue;
        }
        let tdef = &ctx.vehicles[tvi];
        let geo = geometry(ctx, k, t, tr.visible_fraction);
        let r = geo.range;
        if !s.sprung && r > trigger {
            continue;
        }
        // Rounds in reach: the first meant for the target's class that defeats it, else the best.
        let usable: Vec<usize> = (0..st.ammo.len())
            .filter(|&li| {
                let a = &ctx.ammo[st.ammo[li]];
                st.stock[li] > 0
                    && a.max_range_m.as_ref().is_none_or(|m| r <= m.get())
                    && a.min_range_m.as_ref().is_none_or(|m| r >= m.get())
            })
            .collect();
        let margin =
            |li: usize| gunnery::round_margin(&ctx.ammo[st.ammo[li]], ctx.pen, tdef, w.era[t], &geo, top_above);
        let pick = usable
            .iter()
            .copied()
            .find(|&li| ctx.ammo[st.ammo[li]].preferred_vs.contains(&tdef.class.into()) && margin(li) >= 1.0)
            .or_else(|| usable.iter().copied().max_by(|&a, &b| margin(a).total_cmp(&margin(b)).then(b.cmp(&a))));
        let Some(li) = pick else { continue };
        let m = margin(li);
        let flank = gunnery::face_for(&geo, &ctx.ammo[st.ammo[li]], top_above) != Face::Front;
        // Hold on a front the rounds cannot defeat until the flank shows or the ambush is sprung.
        if amb.prefer_flank && !s.sprung && !flank && m < 1.0 {
            continue;
        }
        let c = Choice { target: t, li, margin: m.min(2.0), flank, range: r };
        let better = |b: &Choice| {
            c.flank
                .cmp(&b.flank)
                .then(c.margin.total_cmp(&b.margin))
                .then(b.range.total_cmp(&c.range))
                .then(b.target.cmp(&c.target))
                .is_gt()
        };
        if best.as_ref().is_none_or(better) {
            best = Some(c);
        }
    }
    best
}

/// One tick of squad anti-armour fire. Returns the shots fired.
pub fn step(ctx: &mut Ctx) -> Vec<FireEvent> {
    let mut out = Vec::new();
    for k in 0..ctx.world.len() {
        let Some(s) = ctx.world.squad[k].as_mut() else { continue };
        if s.anti_armour.is_empty() {
            continue;
        }
        for w in s.anti_armour.iter_mut() {
            w.cooldown = (w.cooldown - DT).max(0.0);
        }
        if ctx.world.suppression[k] > SPRUNG_SUPPRESSION {
            s.sprung = true;
        }
        let s = ctx.world.squad[k].as_ref().expect("squad");
        if !ctx.world.alive(k) || s.mounted_in.is_some() || s.speed > 0.1 || s.strength == 0 {
            continue;
        }
        let min_level = match ctx.world.weapons[k] {
            WeaponsStatus::Hold => continue,
            WeaponsStatus::Tight => ContactLevel::Identified,
            WeaponsStatus::Free => ContactLevel::Recognised,
        };
        for wi in 0..s.anti_armour.len() {
            let s = ctx.world.squad[k].as_ref().expect("squad");
            let st = &s.anti_armour[wi];
            let def = &ctx.squads[s.def].anti_armour[st.def];
            // Launchers still manned: the type's count scaled by the men left.
            let manned = (st.count as u32 * s.strength as u32).div_ceil(s.men.max(1) as u32);
            if st.cooldown > 0.0 || st.rounds() == 0 || manned == 0 || s.halted_s < def.setup_s.get() {
                continue;
            }
            let Some(c) = choose(ctx, k, wi, min_level) else { continue };
            if let Some(f) = fire(ctx, k, wi, &c) {
                out.push(f);
            }
        }
    }
    out
}

fn fire(ctx: &mut Ctx, k: usize, wi: usize, c: &Choice) -> Option<FireEvent> {
    let t = c.target;
    let visible = ctx.world.sensors[k].as_ref().and_then(|s| s.track(t)).map_or(0.0, |tr| tr.visible_fraction);
    let geo = geometry(ctx, k, t, visible);
    let sup = ctx.world.suppression[k];
    let s = ctx.world.squad[k].as_ref().expect("squad");
    let st = &s.anti_armour[wi];
    let def = &ctx.squads[s.def].anti_armour[st.def];
    let a = &ctx.ammo[st.ammo[c.li]];
    // A suppressed gunner aims worse.
    let mut fc = def.sight.fire_control();
    fc.aim_error_mrad.value *= 1.0 + sup;
    fc.moving_error_mrad.value *= 1.0 + sup;
    let bs = gunnery::bias_sigma_mrad(&fc, a, geo.range);
    let bias = (gauss(ctx.rng) * bs, gauss(ctx.rng) * bs * 0.5);
    let tdef = &ctx.vehicles[ctx.world.vehicle[t].expect("vehicle")];
    let outcome = gunnery::resolve(&fc, tdef, ctx.world.era[t], a, ctx.pen, ctx.dmg, &geo, bias, ctx.rng);
    let tof = gunnery::time_of_flight(a, geo.range);
    let miss_offset = if outcome.hit { [0.0, 0.0] } else { [gauss(ctx.rng) * 4.0, gauss(ctx.rng) * 4.0 + 5.0] };
    let operator = (a.guided && !a.fire_and_forget).then_some(k);
    let (reload, signature) = (def.reload_s.get(), def.signature_factor.get());
    let from = ctx.world.transform[k].pos;
    let tp = ctx.world.transform[t].pos;
    let s = ctx.world.squad[k].as_mut().expect("squad");
    let st = &mut s.anti_armour[wi];
    st.stock[c.li] -= 1;
    st.cooldown = reload * (1.0 + ctx.slow_gain * sup);
    s.sprung = true;
    ctx.world.last_fired[k] = Some(ctx.tick);
    ctx.world.launch[k] = Some((ctx.tick, signature));
    Some(FireEvent {
        shooter: k,
        projectile: Projectile {
            impact_tick: ctx.tick + (tof / DT).ceil() as u64,
            shooter: k,
            operator,
            guidance_lost: false,
            from,
            to: [tp[0] + miss_offset[0], tp[1] + miss_offset[1]],
            effect: Effect::Shot { target: t, outcome },
        },
    })
}

/// Operator in the loop: a command-guided missile whose gunner is pinned (`pinned_above`) or lost
/// before impact misses (Sagger 1973: crews fired at every launch point; plan §2.2).
pub fn track_guidance(projectiles: &mut [Projectile], world: &World, pinned_above: f64) {
    for p in projectiles.iter_mut() {
        if let Some(op) = p.operator
            && (!world.alive(op) || world.suppression[op] > pinned_above)
        {
            p.guidance_lost = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_chacha::rand_core::SeedableRng;
    use sim_data::{DataSet, FireControlDef};

    fn ds() -> DataSet {
        DataSet::load(&std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")).unwrap()
    }

    /// The fire control of `squad`'s launcher in service in `year` that fires `round`.
    fn sight(ds: &DataSet, squad: &str, year: i32, round: &str) -> FireControlDef {
        let s = ds.squad(squad).unwrap();
        let w = s.anti_armour_in(year).find(|w| w.load.iter().any(|l| l.ammo == round)).unwrap();
        w.sight.fire_control()
    }

    /// First-round hit chance on a `w` × `h` m panel at range `r` (crossing at `v` m/s) through the
    /// sim's error budget: per-shot bias (range estimation through the drop, ballistic error) and
    /// random error (aim, dispersion, lead), as `resolve` samples them.
    fn panel_hit(fc: &FireControlDef, a: &AmmoDef, r: f64, v: f64, w: f64, h: f64) -> f64 {
        let mut rng = ChaCha8Rng::seed_from_u64(9);
        let geo = ShotGeometry {
            range: r,
            visible: 1.0,
            bearing_to_shooter: 0.0,
            target_heading: std::f64::consts::FRAC_PI_2,
            crossing_speed: v,
            shooter_moving: false,
            plunge: 0.0,
        };
        let (sv, sh) = gunnery::random_sigma_mrad(fc, a, &geo);
        let bs = gunnery::bias_sigma_mrad(fc, a, r);
        let n = 20000;
        let hits = (0..n)
            .filter(|_| {
                let ev = (gauss(&mut rng) * bs + gauss(&mut rng) * sv) * r / 1000.0;
                let eh = (gauss(&mut rng) * bs * 0.5 + gauss(&mut rng) * sh) * r / 1000.0;
                ev.abs() <= h / 2.0 && eh.abs() <= w / 2.0
            })
            .count();
        hits as f64 / n as f64
    }

    fn flat(bearing: f64, r: f64) -> ShotGeometry {
        ShotGeometry {
            range: r,
            visible: 1.0,
            bearing_to_shooter: bearing,
            target_heading: 0.0,
            crossing_speed: 0.0,
            shooter_moving: false,
            plunge: 0.0,
        }
    }

    #[test]
    fn the_rpg7_matches_the_published_hit_tables() {
        // Targets A1 and A2 (plan M8B-AT-TEAMS §11), 5 x 2.5 m panel, ± 0.08: Isby (1988),
        // stationary; 1976 US Army evaluation, crossing at 4 m/s (Wikipedia, RPG-7).
        let ds = ds();
        let a = ds.ammo("rkt_pg7v").unwrap();
        let fc = sight(&ds, "mr_squad", 1970, "rkt_pg7v");
        for (r, want) in [(100.0, 0.91), (200.0, 0.52), (300.0, 0.27), (500.0, 0.05)] {
            let p = panel_hit(&fc, a, r, 0.0, 5.0, 2.5);
            assert!((p - want).abs() <= 0.08, "stationary at {r} m: {p:.2} vs {want}");
        }
        for (r, want) in [(100.0, 0.96), (200.0, 0.51), (300.0, 0.22), (400.0, 0.09), (500.0, 0.04)] {
            let p = panel_hit(&fc, a, r, 4.0, 5.0, 2.5);
            assert!((p - want).abs() <= 0.08, "crossing at {r} m: {p:.2} vs {want}");
        }
    }

    #[test]
    fn light_disposables_hit_one_in_two_at_their_effective_range() {
        // Target A3: FM 3-23.25 (2001) via Wikipedia (M72 LAW): beyond the effective range the
        // hit chance is below one half (M72 200 m, AT4 300 m, stationary).
        let ds = ds();
        for (round, year, r) in [("rkt_66", 1980, 200.0), ("rkt_84", 1990, 300.0)] {
            let p = panel_hit(&sight(&ds, "rifle_squad_b", year, round), ds.ammo(round).unwrap(), r, 0.0, 5.0, 2.5);
            assert!((p - 0.5).abs() <= 0.08, "{round} at {r} m: {p:.2}");
        }
    }

    #[test]
    fn a_fire_and_forget_missile_hits_about_nine_in_ten_in_the_open() {
        // Target A9: Javelin class at 500-2,000 m on a tank in the open (DoD, Ukraine 2022: 100 of
        // the first 112 shots hit).
        let ds = ds();
        let a = ds.ammo("atgm_javelin").unwrap();
        let fc = sight(&ds, "at_team_b", 2000, "atgm_javelin");
        let t = ds.vehicle("kurgan_b").unwrap();
        let mut rng = ChaCha8Rng::seed_from_u64(4);
        let (mut hits, mut n) = (0, 0);
        for r in [500.0, 1000.0, 1500.0, 2000.0] {
            for _ in 0..2000 {
                let o =
                    gunnery::resolve(&fc, t, 0, a, &ds.penetration, &ds.damage, &flat(0.0, r), (0.0, 0.0), &mut rng);
                hits += o.hit as u32;
                n += 1;
            }
        }
        let p = hits as f64 / n as f64;
        println!("Javelin hit rate 500-2,000 m: {p:.3}");
        assert!((0.82..=0.95).contains(&p), "{p:.3}");
    }

    #[test]
    fn each_launcher_defeats_the_flanks_it_was_built_for_but_not_every_front() {
        // Per-pairing checks (plan M8B-AT-TEAMS §10.5) at 300 m, by expected margin
        // (penetration / armour, ERA weighted by coverage): the RPG-7 and Dragon against the
        // T-72A, the AT4 against the BMP-2, the Fagot and Kornet against the M1 family, the
        // Javelin from the top against the T-90A.
        let ds = ds();
        let margin = |round: &str, target: &str, era: u32, bearing: f64| {
            let (a, t) = (ds.ammo(round).unwrap(), ds.vehicle(target).unwrap());
            let top = ds.damage.top_hit_above_deg.get().to_radians();
            gunnery::round_margin(a, &ds.penetration, t, era, &flat(bearing, 300.0), top)
        };
        let (front, side) = (0.0, std::f64::consts::FRAC_PI_2);
        for (round, target, era) in [
            ("rkt_pg7v", "kurgan_a", 0),
            ("atgm_dragon", "kurgan_a", 0),
            ("atgm_dragon2", "kurgan_b", 10),
            ("atgm_fagot", "bulwark_a0", 0),
            ("atgm_fagot_m", "bulwark_a1", 0),
            ("atgm_kornet", "bulwark_a2", 0),
        ] {
            let (f, s) = (margin(round, target, era, front), margin(round, target, era, side));
            assert!(f < 1.0 && s > 2.0, "{round} vs {target}: front {f:.2}, side {s:.2}");
        }
        assert!(margin("rkt_84", "skif_m1", 0, front) > 5.0, "the AT4 defeats a BMP-2 from any side");
        assert!(margin("rkt_66", "skif_m0", 0, front) > 5.0, "so does the M72 a BMP-1");
        // Tandem: the PG-7VR keeps its full penetration against the T-72B's reactive armour.
        assert!(margin("rkt_pg7vr", "kurgan_b", 10, side) > margin("rkt_pg7vl", "kurgan_b", 10, side));
        assert!(margin("atgm_javelin", "kurgan_c", 10, front) > 5.0, "top attack meets the roof");
    }
}
