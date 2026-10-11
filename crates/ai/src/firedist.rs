//! Platoon fire distribution (ATP 3-20.15 (2025) ch. 4, fire distribution and control):
//! the frontal fire pattern — tanks engage targets on their own side of the formation so that
//! fires spread across the enemy instead of massing on the nearest target.

use crate::picture::Vec2;

/// Returns, for each shooter, the index of its target. Shooters and targets are ordered
/// left-to-right across the axis from the shooters' centroid to the targets' centroid and
/// paired in order (every target covered when shooters ≥ targets).
pub fn frontal(shooters: &[Vec2], targets: &[Vec2]) -> Vec<usize> {
    if shooters.is_empty() || targets.is_empty() {
        return Vec::new();
    }
    let c = |v: &[Vec2]| {
        let n = v.len() as f64;
        [v.iter().map(|p| p[0]).sum::<f64>() / n, v.iter().map(|p| p[1]).sum::<f64>() / n]
    };
    let (cs, ct) = (c(shooters), c(targets));
    let axis = crate::drive::bearing(cs, ct);
    let right = [libm::cos(axis), -libm::sin(axis)];
    let lateral = |p: &Vec2| p[0] * right[0] + p[1] * right[1];
    let mut si: Vec<usize> = (0..shooters.len()).collect();
    si.sort_by(|&a, &b| lateral(&shooters[a]).total_cmp(&lateral(&shooters[b])).then(a.cmp(&b)));
    let mut ti: Vec<usize> = (0..targets.len()).collect();
    ti.sort_by(|&a, &b| lateral(&targets[a]).total_cmp(&lateral(&targets[b])).then(a.cmp(&b)));
    let mut out = vec![0; shooters.len()];
    let (ns, nt) = (shooters.len(), targets.len());
    for (rank, &s) in si.iter().enumerate() {
        let t = if ns == 1 { 0 } else { (rank * (nt - 1) + (ns - 1) / 2) / (ns - 1).max(1) };
        out[s] = ti[t.min(nt - 1)];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flank_tanks_take_flank_targets() {
        // Shooters in a line facing north; targets spread west to east.
        let shooters = [[0.0, 0.0], [100.0, 0.0], [200.0, 0.0], [300.0, 0.0]];
        let targets = [[350.0, 2000.0], [-50.0, 2000.0]];
        let a = frontal(&shooters, &targets);
        assert_eq!(a[0], 1, "westernmost tank takes the western target");
        assert_eq!(a[3], 0, "easternmost tank takes the eastern target");
        assert!(a.contains(&0) && a.contains(&1));
    }
}
