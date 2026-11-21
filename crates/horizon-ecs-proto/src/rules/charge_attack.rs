//! Calculations of a charge: the fixed velocity toward the target, and the tests that end the charge before the contact.
use horizon_core::geometry::Vec3;

/// The velocity of a charge from `from` toward `to`: its length is `speed` (zero when the target is on the animal).
pub fn charge_velocity(from: Vec3, to: Vec3, speed: f64) -> Vec3 {
    let (dx, dy, dz) = (to.x - from.x, to.y - from.y, to.z - from.z);
    let len = (dx * dx + dy * dy + dz * dz).sqrt();
    if len < 1.0e-4 {
        return Vec3::new(0.0, 0.0, 0.0);
    }
    Vec3::new(dx / len * speed, dy / len * speed, dz / len * speed)
}

/// Whether the charge ends: the animal is `max_run_sqr` away from its start, the target `max_target_sqr` away, or the target is not seen.
pub fn charge_ends(start: Vec3, pos: Vec3, target: Vec3, seen: bool, max_run_sqr: f64, max_target_sqr: f64) -> bool {
    sq_dist(start, pos) >= max_run_sqr || sq_dist(target, pos) >= max_target_sqr || !seen
}

fn sq_dist(a: Vec3, b: Vec3) -> f64 {
    (a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)
}
