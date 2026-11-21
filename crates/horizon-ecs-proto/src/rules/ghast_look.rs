//! Angles of `GhastLookGoal.tick` (`nether.rs:307-336`): the body faces the target within range, else the motion.
use horizon_core::geometry::trig;

/// The angle the body faces: the target's offset when it lies within `range_sqr`, else the motion; `None` keeps the yaw.
/// `target` is `None` when there is no target to look at.
pub fn look_angle(vel_x: f64, vel_z: f64, target: Option<(f64, f64, f64)>, range_sqr: f64) -> Option<f64> {
    match target {
        None => Some(trig::atan2(vel_x, vel_z)),
        Some((dx, dy, dz)) => (dx * dx + dy * dy + dz * dz < range_sqr).then(|| trig::atan2(dx, dz)),
    }
}

/// The yaw in degrees for an angle from [`look_angle`].
pub fn yaw_degrees(angle: f64) -> f32 {
    -(angle as f32) * (180.0_f32 / std::f32::consts::PI)
}
