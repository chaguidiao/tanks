//! Tracked-vehicle longitudinal and steering dynamics on terrain (DESIGN.md §7.1).
//!
//! Longitudinal (Wong, *Theory of Ground Vehicles*, 5th ed., ch. on tracked-vehicle
//! performance; Ogorkiewicz, *Technology of Tanks*, 1991):
//! - tractive effort `F = min(η·P / v, μ·m·g·cos θ)`: engine power through the drivetrain,
//!   limited by soil traction;
//! - resistance `R = m·g·(f_r·cos θ + sin θ)`: soil motion resistance plus grade;
//! - speed further capped by the terrain cap (vegetation, roads, water) and governed road speed.
//!
//! Steering: skid-steer yaw rate limited by the hull's pivot rate and by lateral acceleration
//! `ω ≤ a_lat / v`. Fuel: idle burn plus brake-specific consumption on delivered power.

use sim_data::{TerrainData, VehicleDef};
use terrain::BakedMap;
use terrain::mobility::{MobilityClass, terrain_cap};

use crate::world::World;
use crate::{DT, Event};

pub const G: f64 = 9.80665;
/// Below this speed, power-limited force is evaluated at this speed (torque/converter limit).
const MIN_POWER_SPEED: f64 = 1.0;
/// Speed controller time constant (s).
const SPEED_TAU: f64 = 1.0;
/// Seconds without progress before a vehicle reports it is stuck.
const STUCK_AFTER_S: f64 = 5.0;
/// Crawl speed when the side slope exceeds the vehicle's limit (m/s).
const SIDE_SLOPE_CRAWL: f64 = 1.0;

/// Steady-state power-limited speed on a grade (the reference formula the model must reproduce).
pub fn power_limited_speed(power_w: f64, efficiency: f64, mass_kg: f64, rolling: f64, grade: f64) -> f64 {
    let th = libm::atan(grade);
    efficiency * power_w / (mass_kg * G * (rolling * libm::cos(th) + libm::sin(th)))
}

pub(crate) fn step(
    world: &mut World,
    vehicles: &[VehicleDef],
    map: &BakedMap,
    td: &TerrainData,
    events: &mut Vec<Event>,
) {
    let hf = &map.elevation;
    for k in 0..world.len() {
        let movable = world.can_move(k);
        let (Some(m), Some(vi)) = (&mut world.mobility[k], world.vehicle[k]) else { continue };
        if !movable {
            m.speed = 0.0;
            continue;
        }
        let def = &vehicles[vi];
        let d = &def.mobility;
        let class = MobilityClass::from(def);
        let t = &mut world.transform[k];

        let (i, j) = hf.cell_of(t.pos[0], t.pos[1]);
        let cap = terrain_cap(&class, td, map.landcover.get(i, j), map.water_depth.get(i, j));
        let soil = td.soil(map.soil.get(i, j));
        let (f_r, mu) = (soil.rolling_resistance.get(), soil.traction_coefficient.get());

        // Steering.
        let w_max = d.max_yaw_rate_rad_s.get().min(d.max_lateral_accel_mps2.get() / m.speed.abs().max(0.1));
        let err = ai::drive::wrap(m.cmd_heading - t.heading);
        let dh = err.clamp(-w_max * DT, w_max * DT);
        t.heading = ai::drive::wrap(t.heading + dh);

        // Terrain geometry along the hull.
        let (sh, ch) = (libm::sin(t.heading), libm::cos(t.heading));
        let (gx, gy) = hf.gradient(t.pos[0], t.pos[1]);
        let grade = gx * sh + gy * ch;
        let side = gx * ch - gy * sh; // positive: ground rises to the left (right side down)
        let th = libm::atan(grade);
        let (cth, sth) = (libm::cos(th), libm::sin(th));

        // Speed target.
        let mut target = m.cmd_speed.min(cap).min(d.max_speed_road_mps.get());
        if side.abs() > d.max_side_slope.get() {
            target = target.min(SIDE_SLOPE_CRAWL);
        }
        if m.fuel_l <= 0.0 {
            target = 0.0;
        }

        // Longitudinal dynamics (forward motion only; reverse comes with drills in M4).
        let mass = d.combat_mass_kg.get();
        let (power, eta) = (d.engine_power_w.get(), d.drivetrain_efficiency.get());
        let resist = mass * G * (f_r * cth + sth);
        let traction = mu * mass * G * cth;
        let f_drive = (eta * power / m.speed.max(MIN_POWER_SPEED)).min(traction);
        let a_max = (f_drive - resist) / mass;
        let a_min = -(d.braking_decel_mps2.get().min(mu * G * cth)) - resist / mass;
        // Accelerate smoothly; brake firmly when well above the target (drivers brake, they
        // do not coast to a halt at a position).
        let a_des = if target < m.speed - 0.5 { -d.braking_decel_mps2.get() } else { (target - m.speed) / SPEED_TAU };
        let a = a_des.min(a_max).max(a_min.min(a_max));
        let v_old = m.speed;
        m.speed = (m.speed + a * DT).max(0.0);
        if a_des < 0.0 && m.speed < target {
            m.speed = target.min(v_old); // braking does not undershoot the target
        }

        // Progress / stuck detection.
        if m.cmd_speed > 0.5 && m.speed < 0.2 {
            let before = m.stalled_s;
            m.stalled_s += DT;
            if before < STUCK_AFTER_S && m.stalled_s >= STUCK_AFTER_S {
                events.push(Event::Stuck { unit: world.id[k] });
            }
        } else {
            m.stalled_s = 0.0;
        }

        // Kinematics (horizontal component of motion along the slope).
        let v_h = m.speed * cth;
        let (w_m, h_m) = (hf.width_m(), hf.height_m());
        t.pos[0] = (t.pos[0] + sh * v_h * DT).clamp(0.0, w_m);
        t.pos[1] = (t.pos[1] + ch * v_h * DT).clamp(0.0, h_m);
        t.z = hf.height(t.pos[0], t.pos[1]);
        t.pitch = th;
        t.roll = libm::atan(side);

        // Fuel: idle plus delivered power.
        let p_wheel = ((mass * a + resist) * m.speed).max(0.0);
        let p_engine = (p_wheel / eta).min(power);
        let lph = d.fuel_idle_lph.get() + p_engine / 1000.0 * d.fuel_per_kwh_l.get();
        let had_fuel = m.fuel_l > 0.0;
        m.fuel_l = (m.fuel_l - lph / 3600.0 * DT).max(0.0);
        if had_fuel && m.fuel_l <= 0.0 {
            events.push(Event::OutOfFuel { unit: world.id[k] });
        }
    }
}
