//! Control measures and contingency orders (DESIGN.md §8.2–8.4): phase lines, objectives,
//! TRPs and battle positions per side, and triggers that release a pre-planned order without
//! radio delay ("code word") when a condition is met.

use ai::picture::{Graphic, GraphicKind, PerceivedWorld, TriggerCondition, TriggerView};

use crate::Vec2;
use crate::command::Command;

#[derive(Debug, Clone)]
pub struct Trigger {
    pub id: u32,
    pub condition: TriggerCondition,
    pub then: Command,
    pub fired: bool,
    /// Side of the line (sign) the friendly force was on when the trigger was set.
    pub reference_sign: f64,
}

/// Signed distance of `p` to a polyline (positive on the left of the direction of drawing)
/// and whether its projection falls within the line's span.
pub fn side_of_line(line: &[Vec2], p: Vec2) -> (f64, bool) {
    let mut best = (f64::INFINITY, 0.0, false);
    for (k, w) in line.windows(2).enumerate() {
        let (a, b) = (w[0], w[1]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l2 = dx * dx + dy * dy;
        if l2 == 0.0 {
            continue;
        }
        let t = ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2;
        let tc = t.clamp(0.0, 1.0);
        let c = [a[0] + dx * tc, a[1] + dy * tc];
        let d = libm::sqrt((p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2));
        if d < best.0 {
            let cross = dx * (p[1] - a[1]) - dy * (p[0] - a[0]);
            let first = k == 0;
            let last = k + 2 == line.len();
            let inside = (t >= 0.0 || !first) && (t <= 1.0 || !last);
            best = (d, cross.signum(), inside);
        }
    }
    (best.1, best.2)
}

pub fn line_points(graphics: &[Graphic], id: u32) -> Option<&[Vec2]> {
    graphics.iter().find(|g| g.id == id).and_then(|g| match &g.kind {
        GraphicKind::PhaseLine { points } => Some(points.as_slice()),
        _ => None,
    })
}

fn centroid(pic: &PerceivedWorld, platoon: Option<ai::picture::PlatoonId>) -> Option<Vec2> {
    let units: Vec<_> = pic
        .own
        .iter()
        .filter(|u| platoon.is_none_or(|p| u.platoon == Some(p)) && u.condition != ai::picture::Condition::Destroyed)
        .collect();
    if units.is_empty() {
        return None;
    }
    let n = units.len() as f64;
    Some([units.iter().map(|u| u.pos[0]).sum::<f64>() / n, units.iter().map(|u| u.pos[1]).sum::<f64>() / n])
}

/// Reference sign for a new trigger: which side of the line the relevant friendly force is on.
pub fn reference_sign(pic: &PerceivedWorld, c: &TriggerCondition) -> f64 {
    match c {
        TriggerCondition::EnemyCrosses { line } => line_points(&pic.graphics, *line)
            .and_then(|l| centroid(pic, None).map(|p| side_of_line(l, p).0))
            .unwrap_or(1.0),
        TriggerCondition::FriendlyCrosses { line, platoon } => line_points(&pic.graphics, *line)
            .and_then(|l| centroid(pic, Some(*platoon)).map(|p| side_of_line(l, p).0))
            .unwrap_or(1.0),
        TriggerCondition::AtTime { .. } => 1.0,
    }
}

/// Is the trigger's condition met in the side's picture?
pub fn met(pic: &PerceivedWorld, t: &Trigger) -> bool {
    match &t.condition {
        TriggerCondition::AtTime { time_s } => pic.time_s >= *time_s,
        TriggerCondition::EnemyCrosses { line } => {
            let Some(l) = line_points(&pic.graphics, *line) else { return false };
            pic.live_contacts().filter(|c| pic.time_s - c.last_report_s < 30.0).any(|c| {
                let (s, inside) = side_of_line(l, c.pos);
                inside && s == t.reference_sign
            })
        }
        TriggerCondition::FriendlyCrosses { line, platoon } => {
            let Some(l) = line_points(&pic.graphics, *line) else { return false };
            centroid(pic, Some(*platoon)).is_some_and(|p| side_of_line(l, p).0 != t.reference_sign)
        }
    }
}

pub fn describe(c: &Command) -> String {
    match c {
        Command::PlatoonMove { .. } => "MOVE".into(),
        Command::OccupyPosition { .. } => "OCCUPY BP".into(),
        Command::Halt { .. } => "HALT".into(),
        Command::Resume { .. } => "RESUME".into(),
        Command::SetSpeed { .. } => "SPEED".into(),
        Command::WeaponsControl { status, .. } => format!("WEAPONS {status:?}").to_uppercase(),
        Command::Move { .. } => "MOVE".into(),
        Command::SetFormation { formation, .. } => format!("FORMATION {formation:?}").to_uppercase(),
        _ => "ORDER".into(),
    }
}

pub fn views(triggers: &[Trigger]) -> Vec<TriggerView> {
    triggers
        .iter()
        .map(|t| TriggerView { id: t.id, condition: t.condition.clone(), order: describe(&t.then), fired: t.fired })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn side_of_line_sign_and_span() {
        let l = [[0.0, 0.0], [100.0, 0.0]];
        assert_eq!(side_of_line(&l, [50.0, 10.0]), (1.0, true));
        assert_eq!(side_of_line(&l, [50.0, -10.0]), (-1.0, true));
        assert!(!side_of_line(&l, [150.0, 10.0]).1, "beyond the end is outside the span");
    }
}
