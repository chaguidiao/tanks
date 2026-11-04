//! Recovery of immobilised vehicles (docs/plans/M8C-RECOVERY.md; FM 17-15 (1996) and FM 71-1
//! (1998): what the crew cannot fix or move is recovered by the maintenance section's recovery
//! vehicle to the unit maintenance collection point (UMCP) and repaired there; FM 20-22 (1988)
//! on towing with a tow bar). The job logic runs in the platoon phase
//! (`platoon_ai::recover_step`); this module holds the state, the tow bar and the repairs.

use ai::picture::{Condition, PlatoonId, RecoveryStage};
use terrain::ground::Ground;
use terrain::hash::StableHasher;

use crate::Vec2;
use crate::world::World;

/// Tow gear of a recovery vehicle; `Some` in `World::tow` marks one.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TowGear {
    /// The vehicle on the tow bar (world index).
    pub towing: Option<usize>,
}

/// Where one recovery vehicle is in its work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryStep {
    /// Nothing taken: at or driving back to the collection point.
    Idle,
    /// Driving to the vehicle it has taken.
    ToVehicle,
    /// Halted behind it, rigging the tow bar, until `until`.
    HookingUp,
    /// Towing it to the collection point.
    Towing,
    /// Halted at the collection point, dropping the tow, until `until`.
    Unhooking,
}

/// One recovery vehicle's work (plan M8C-RECOVERY §3).
#[derive(Debug, Clone, PartialEq)]
pub struct RecoveryJob {
    /// Index into the order's `targets` of the vehicle it has taken.
    pub target: Option<usize>,
    pub step: RecoveryStep,
    /// Tick the current halt (hooking up, unhooking) ends.
    pub until: u64,
    /// Where it was last routed (NaN = not yet).
    pub goal: Vec2,
}

impl RecoveryJob {
    pub fn new() -> Self {
        Self { target: None, step: RecoveryStep::Idle, until: 0, goal: [f64::NAN, f64::NAN] }
    }

    pub(crate) fn hash_into(&self, h: &mut StableHasher) {
        h.u64(self.target.map_or(u64::MAX, |v| v as u64));
        h.u8(self.step as u8);
        h.u64(self.until);
    }
}

impl Default for RecoveryJob {
    fn default() -> Self {
        Self::new()
    }
}

/// A recovered vehicle being repaired at the collection point.
#[derive(Debug, Clone, PartialEq)]
pub struct Repair {
    /// World index.
    pub k: usize,
    pub platoon: Option<PlatoonId>,
    pub done_tick: u64,
}

/// The tow bar, every tick after mobility: the towed vehicle trails `hitch_m` behind the
/// recovery vehicle like a trailer, rear first (it was hooked up from behind), at its speed.
pub fn tow_step(world: &mut World, ground: &Ground, hitch_m: f64) {
    for k in 0..world.len() {
        let Some(t) = world.tow[k].as_ref().and_then(|g| g.towing) else { continue };
        let p = world.transform[k].pos;
        let q = world.transform[t].pos;
        let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
        let d = (dx * dx + dy * dy).sqrt();
        let (ux, uy) = if d > 1e-6 {
            (dx / d, dy / d)
        } else {
            let h = world.transform[k].heading;
            (-libm::sin(h), -libm::cos(h))
        };
        let pos = [p[0] + ux * hitch_m, p[1] + uy * hitch_m];
        let speed = world.mobility[k].as_ref().map_or(0.0, |m| m.speed);
        let tr = &mut world.transform[t];
        tr.pos = pos;
        tr.z = ground.height(pos[0], pos[1]);
        // Rear towards the recovery vehicle: the hull points away from it.
        tr.heading = libm::atan2(ux, uy);
        if let Some(m) = world.mobility[t].as_mut() {
            m.speed = speed;
        }
    }
}

/// Drop whatever vehicle `k` has on the tow bar, where it is.
pub fn drop_tow(world: &mut World, k: usize) {
    if let Some(t) = world.tow[k].as_mut().and_then(|g| g.towing.take())
        && let Some(m) = world.mobility[t].as_mut()
    {
        m.speed = 0.0;
    }
}

/// Repairs finished at `tick`: the vehicle is operational again. Returns the vehicles repaired;
/// repairs of vehicles destroyed meanwhile are dropped.
pub fn repair_step(world: &mut World, repairs: &mut Vec<Repair>, tick: u64) -> Vec<Repair> {
    repairs.retain(|r| world.condition[r.k] == Condition::MobilityKill);
    let (done, keep): (Vec<_>, Vec<_>) = std::mem::take(repairs).into_iter().partition(|r| tick >= r.done_tick);
    *repairs = keep;
    for r in &done {
        world.condition[r.k] = Condition::Operational;
    }
    done
}

/// Picture stage of a vehicle in repair, if it is.
pub fn repair_stage(repairs: &[Repair], k: usize, tick: u64) -> Option<RecoveryStage> {
    repairs
        .iter()
        .find(|r| r.k == k)
        .map(|r| RecoveryStage::InRepair { ready_in_s: r.done_tick.saturating_sub(tick) as f64 * crate::DT })
}
