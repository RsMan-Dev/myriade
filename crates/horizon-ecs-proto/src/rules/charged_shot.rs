//! Calculations of a charged shot: the velocity of the fireball from its launch point toward the aim point.
use horizon_core::geometry::Vec3;

/// The velocity of a fireball from `from` toward `to`: a length of 0.1 (the speed of an `AbstractHurtingProjectile`).
pub fn shot_velocity(from: Vec3, to: Vec3) -> Vec3 {
    let dir = Vec3::new(to.x - from.x, to.y - from.y, to.z - from.z);
    let len = (dir.x * dir.x + dir.y * dir.y + dir.z * dir.z).sqrt().max(1.0e-9);
    Vec3::new(dir.x / len * 0.1, dir.y / len * 0.1, dir.z / len * 0.1)
}
