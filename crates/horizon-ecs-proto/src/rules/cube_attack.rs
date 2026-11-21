//! Calculations of a cube's attack: the body yaw that turns it toward its target.
use horizon_core::geometry::{trig, Vec3};

/// The body yaw that faces `at` from `from` (`atan2` in degrees, less 90 as the `Mob`'s `turn_to`).
pub fn body_yaw_toward(from: Vec3, at: Vec3) -> f32 {
    trig::atan2_degrees(at.z - from.z, at.x - from.x) - 90.0
}
