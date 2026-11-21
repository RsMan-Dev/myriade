//! The dolphin's leap: the swim direction from the yaw, the take-off impulse, and the pitch of the leap.
use crate::cow::wrap;
use horizon_core::geometry::Vec3;

/// The swim direction of the yaw: south, west, north or east (`motion_step`).
pub fn motion_step(yaw: f32) -> (i32, i32) {
    match ((f64::from(yaw) / 90.0 + 0.5).floor() as i32) & 3 {
        0 => (0, 1),
        1 => (-1, 0),
        2 => (0, -1),
        _ => (1, 0),
    }
}

/// The take-off impulse: the push along the swim direction, and the lift.
pub fn jump_impulse(step: (i32, i32), push: f64, lift: f64) -> Vec3 {
    Vec3::new(f64::from(step.0) * push, lift, f64::from(step.1) * push)
}

/// The pitch of the tick, in degrees: back towards 0 when the climb is weak, else the angle of the velocity.
pub fn next_pitch(v: Vec3, pitch: f32, pitch_threshold: f64, lerp: f32) -> f32 {
    let horizontal = (v.x * v.x + v.z * v.z).sqrt();
    if v.y * v.y < pitch_threshold && pitch != 0.0 {
        pitch + lerp * wrap(0.0 - pitch)
    } else if (v.x * v.x + v.y * v.y + v.z * v.z).sqrt() > 1e-5 {
        (-v.y).atan2(horizontal).to_degrees() as f32
    } else {
        pitch
    }
}
