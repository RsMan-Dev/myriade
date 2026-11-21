//! Calculs de la grimpe : l'animal est-il au centre de la case visée (à sa largeur près), et doit-il encore grimper vers elle.
use horizon_core::geometry::Vec3;

/// `WallClimberNavigation`: the animal at `p`, `width` wide, is within its width of the centre of the block `cell`.
pub fn at_centre(p: Vec3, width: f64, cell: [i32; 3]) -> bool {
    (f64::from(cell[0]) + 0.5 - p.x).powi(2) + (f64::from(cell[1]) + 0.5 - p.y).powi(2) + (f64::from(cell[2]) + 0.5 - p.z).powi(2) < width * width
}
/// `WallClimberNavigation.tick`: the climb goes on toward `t` while `t` is not reached (at its centre, or at its level on the same column).
pub fn keep_climbing(p: Vec3, width: f64, t: [i32; 3]) -> bool {
    let same_level = [t[0], p.y.floor() as i32, t[2]];
    !at_centre(p, width, t) && (p.y <= f64::from(t[1]) || !at_centre(p, width, same_level))
}
