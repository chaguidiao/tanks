//! Victory conditions (DESIGN.md §2.1, scenario format): a side that loses its fraction of
//! tanks loses; an attacker holding the objective with no defender inside for
//! `SEIZE_HOLD_S` wins; at the time limit the objective holder wins, ties going to the defender.

use ai::picture::{Condition, SideId};
use sim_data::{ScenarioDef, VictoryDef};

use crate::command::EndReason;
use crate::dist;
use crate::world::World;

/// Seconds the attacker must hold the objective uncontested.
pub const SEIZE_HOLD_S: f64 = 120.0;

#[derive(Debug, Default, Clone)]
pub struct VictoryState {
    held_since: Option<f64>,
}

fn fighting(w: &World, k: usize) -> bool {
    matches!(w.condition[k], Condition::Operational | Condition::MobilityKill)
}

impl VictoryState {
    pub fn check(
        &mut self,
        w: &World,
        sc: &ScenarioDef,
        v: &VictoryDef,
        now: f64,
    ) -> Option<(Option<SideId>, EndReason)> {
        let n_sides = sc.sides.len();
        let other = |s: usize| SideId(((s + 1) % n_sides) as u8);
        for s in 0..n_sides {
            let total = (0..w.len()).filter(|&k| w.side[k].0 as usize == s).count();
            if total == 0 {
                continue;
            }
            let lost = (0..w.len()).filter(|&k| w.side[k].0 as usize == s && !fighting(w, k)).count();
            if lost as f64 / total as f64 >= v.destroyed_fraction {
                return Some((Some(other(s)), EndReason::ForceDestroyed { side: SideId(s as u8) }));
            }
        }
        let holder =
            v.objective.as_ref().and_then(|name| sc.objectives.iter().find(|o| &o.name == name)).and_then(|o| {
                let inside = |s: usize| {
                    (0..w.len()).any(|k| {
                        w.side[k].0 as usize == s && fighting(w, k) && dist(w.transform[k].pos, o.centre) <= o.radius_m
                    })
                };
                let present: Vec<usize> = (0..n_sides).filter(|&s| inside(s)).collect();
                (present.len() == 1).then(|| SideId(present[0] as u8))
            });
        let attacker = SideId(v.attacker as u8);
        if holder == Some(attacker) {
            let since = *self.held_since.get_or_insert(now);
            if now - since >= SEIZE_HOLD_S {
                return Some((Some(attacker), EndReason::ObjectiveSeized { side: attacker }));
            }
        } else {
            self.held_since = None;
        }
        if now >= v.time_limit_s {
            let winner = if holder == Some(attacker) { attacker } else { other(v.attacker) };
            return Some((Some(winner), EndReason::TimeLimit { holder }));
        }
        None
    }
}
