//! Calcul de la fuite après un coup (`HoglinAi.retreatFromNearestTarget`) : distance au carré entre deux points.
use horizon_core::geometry::Vec3;

/// The squared distance from `me` to `p`: the criterion of the nearest attacker, avoid target or attack target.
pub fn dist_sq(p: Vec3, me: Vec3) -> f64 {
    (p.x - me.x).powi(2) + (p.y - me.y).powi(2) + (p.z - me.z).powi(2)
}
