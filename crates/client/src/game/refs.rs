//! Report positions relative to the side's own control measures ("300 M BEYOND PL AMBER",
//! "NEAR TRP 2"), so a report reads against the plan and not only as a grid. Reads the
//! picture's graphics and own units only.

use ai::picture::{Graphic, GraphicKind, PerceivedWorld};

use crate::format;

const TRP_NEAR_M: f64 = 400.0;
const PL_NEAR_M: f64 = 1500.0;

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// Distance to a polyline and the side (+1 left of the drawing direction, -1 right) of the
/// nearest point on it.
fn line_offset(line: &[[f64; 2]], p: [f64; 2]) -> Option<(f64, f64)> {
    let mut best: Option<(f64, f64)> = None;
    for w in line.windows(2) {
        let (a, b) = (w[0], w[1]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l2 = dx * dx + dy * dy;
        if l2 == 0.0 {
            continue;
        }
        let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0);
        let d = dist(p, [a[0] + dx * t, a[1] + dy * t]);
        if best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, (dx * (p[1] - a[1]) - dy * (p[0] - a[0])).signum()));
        }
    }
    best
}

/// "AT TRP 1" / "NEAR TRP 1" within 400 m of a TRP, else the nearest phase line within
/// 1.5 km as "SHORT OF" / "BEYOND" (the side the own force is on is "short"), else `None`.
pub fn relative_to_picture(pic: &PerceivedWorld, p: [f64; 2]) -> Option<String> {
    let own: Vec<[f64; 2]> = pic.own.iter().map(|u| u.pos).collect();
    relative_to_graphics(&pic.graphics, &own, p)
}

fn relative_to_graphics(graphics: &[Graphic], own: &[[f64; 2]], p: [f64; 2]) -> Option<String> {
    let trp = graphics
        .iter()
        .filter_map(|g| match &g.kind {
            GraphicKind::Trp { at } => Some((dist(*at, p), &g.label)),
            _ => None,
        })
        .min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((d, label)) = trp.filter(|t| t.0 <= TRP_NEAR_M) {
        return Some(format!("{} {label}", if d <= 100.0 { "AT" } else { "NEAR" }));
    }
    let n = own.len() as f64;
    if n == 0.0 {
        return None;
    }
    let centre = own.iter().fold([0.0, 0.0], |c, u| [c[0] + u[0] / n, c[1] + u[1] / n]);
    graphics
        .iter()
        .filter_map(|g| match &g.kind {
            GraphicKind::PhaseLine { points } => {
                let (d, side) = line_offset(points, p)?;
                let (_, own) = line_offset(points, centre)?;
                Some((d, side == own, &g.label))
            }
            _ => None,
        })
        .filter(|(d, ..)| *d <= PL_NEAR_M)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(d, short, label)| {
            let d = (d / 100.0).round() * 100.0;
            if d < 100.0 {
                format!("ON {label}")
            } else {
                format!("{} {} {label}", format::distance(d), if short { "SHORT OF" } else { "BEYOND" })
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pl(points: Vec<[f64; 2]>) -> Graphic {
        Graphic { id: 1, label: "PL AMBER".into(), kind: GraphicKind::PhaseLine { points } }
    }

    #[test]
    fn beyond_and_short_of_a_phase_line() {
        // West-east line drawn left to right; own force south of it.
        let g = [pl(vec![[0.0, 2000.0], [4000.0, 2000.0]])];
        let own = [[2000.0, 1000.0]];
        assert_eq!(relative_to_graphics(&g, &own, [2000.0, 2700.0]).as_deref(), Some("700 M BEYOND PL AMBER"));
        assert_eq!(relative_to_graphics(&g, &own, [2000.0, 1500.0]).as_deref(), Some("500 M SHORT OF PL AMBER"));
        assert_eq!(relative_to_graphics(&g, &own, [2000.0, 5000.0]), None);
        assert_eq!(relative_to_graphics(&g, &own, [2000.0, 2030.0]).as_deref(), Some("ON PL AMBER"));
    }

    #[test]
    fn trp_wins_when_near() {
        let t = [Graphic { id: 2, label: "TRP 2".into(), kind: GraphicKind::Trp { at: [1000.0, 1000.0] } }];
        let own = [[0.0, 0.0]];
        assert_eq!(relative_to_graphics(&t, &own, [1050.0, 1000.0]).as_deref(), Some("AT TRP 2"));
        assert_eq!(relative_to_graphics(&t, &own, [1300.0, 1000.0]).as_deref(), Some("NEAR TRP 2"));
        assert_eq!(relative_to_graphics(&t, &own, [3000.0, 1000.0]), None);
    }
}
