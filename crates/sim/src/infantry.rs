//! Dismounted infantry (docs/plans/M8B-INFANTRY.md; DESIGN.md §14 #29): squads riding in and
//! leaving their carriers, losses, and what happens to the men when the carrier is hit.

use ai::picture::Condition;
use rand_chacha::ChaCha8Rng;
use sim_data::InfantryModel;

use crate::Event;
use crate::command::KillKind;
use crate::sensors::uniform;
use crate::world::{Posture, World};

/// Mounted squads ride with their carrier: same position and heading (plan §2).
pub fn follow_carriers(world: &mut World) {
    for k in 0..world.len() {
        let Some(c) = world.squad[k].as_ref().and_then(|s| s.mounted_in) else { continue };
        world.transform[k] = world.transform[c];
    }
}

/// Take `lost` men off squad `k`. Below the doctrine's threshold the squad is combat-ineffective;
/// with no men left it is destroyed.
pub fn casualties(
    world: &mut World,
    k: usize,
    lost: u8,
    by: Option<usize>,
    ineffective_fraction: f64,
    events: &mut Vec<Event>,
) {
    let Some(s) = world.squad[k].as_mut() else { return };
    let lost = lost.min(s.strength);
    if lost == 0 {
        return;
    }
    s.strength -= lost;
    if (s.strength as f64) < ineffective_fraction * s.men as f64 {
        s.ineffective = true;
    }
    let left = s.strength;
    if left == 0 {
        world.condition[k] = Condition::Destroyed;
        s.target = None;
        s.speed = 0.0;
        if let Some(c) = s.mounted_in.take()
            && let Some(cs) = world.carrier[c].as_mut()
        {
            cs.embarked.retain(|&q| q != k);
        }
    }
    events.push(Event::Casualties { unit: world.id[k], by: by.map(|b| world.id[b]), lost, left });
}

/// Squads in a carrier share its fate (plan §2): a penetration costs a fraction of the men, a
/// catastrophic kill the data's (larger) fraction. If the carrier is destroyed, the survivors
/// bail out beside it.
#[allow(clippy::too_many_arguments)]
pub fn carrier_hit(
    world: &mut World,
    carrier: usize,
    penetrated: bool,
    kill: Option<KillKind>,
    model: &InfantryModel,
    ineffective: &[f64],
    by: Option<usize>,
    rng: &mut ChaCha8Rng,
    events: &mut Vec<Event>,
) {
    let Some(cs) = world.carrier[carrier].as_ref() else { return };
    if cs.embarked.is_empty() || !penetrated {
        return;
    }
    let destroyed = world.condition[carrier] == Condition::Destroyed;
    let p = if destroyed || kill == Some(KillKind::Catastrophic) {
        model.mounted_loss.catastrophic.get()
    } else {
        model.mounted_loss.penetration.get()
    };
    for k in cs.embarked.clone() {
        let Some(s) = world.squad[k].as_ref() else { continue };
        let lost = (0..s.strength).filter(|_| uniform(rng) < p).count() as u8;
        let thr = ineffective[world.side[k].0 as usize];
        casualties(world, k, lost, by, thr, events);
        if destroyed {
            bail_out(world, carrier, k);
        }
    }
    if destroyed && let Some(cs) = world.carrier[carrier].as_mut() {
        cs.embarked.clear();
        cs.dismounting = None;
    }
}

/// Put a squad on the ground behind its carrier (the ramp or doors are at the rear).
pub fn bail_out(world: &mut World, carrier: usize, k: usize) {
    let t = world.transform[carrier];
    let Some(s) = world.squad[k].as_mut() else { return };
    s.mounted_in = None;
    s.posture = Posture::Halted;
    s.halted_s = 0.0;
    s.boarding = None;
    let (sh, ch) = (libm::sin(t.heading), libm::cos(t.heading));
    world.transform[k].pos = [t.pos[0] - sh * 6.0, t.pos[1] - ch * 6.0];
}
