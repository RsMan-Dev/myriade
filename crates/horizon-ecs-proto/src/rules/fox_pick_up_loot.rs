//! The fox's looting: when its mouth can hold an item, the spot ahead of the mouth, and the throw of a dropped stack.
use horizon_core::geometry::{trig, Vec3};

/// `Fox.canHoldItem`: an empty mouth, or food to swap for what is not food once it has eaten at all (`ticks` since it ate).
pub fn can_hold_item(mouth_empty: bool, ticks: u64, item_food: bool, held_food: bool) -> bool {
    mouth_empty || (ticks > 0 && item_food && !held_food)
}
/// The offset `(x, z)` of the spot ahead of the mouth, from the yaw and the pitch in degrees.
pub fn mouth_offset(yaw: f32, pitch: f32) -> (f64, f64) {
    let (yr, pr) = (-yaw * trig::DEG_TO_RAD, pitch * trig::DEG_TO_RAD);
    (f64::from(trig::sin(f64::from(yr)) * trig::cos(f64::from(pr))), f64::from(trig::cos(f64::from(yr)) * trig::cos(f64::from(pr))))
}
/// `ItemEntity`'s constructor: the motion of a thrown stack from two draws `a`, `b` in [0, 1).
pub fn toss_velocity(a: f64, b: f64) -> Vec3 {
    Vec3::new(a * 0.2 - 0.1, 0.2, b * 0.2 - 0.1)
}
