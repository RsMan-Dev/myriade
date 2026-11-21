//! The throw of a half stack, and the toss of the thrown stack toward the other villager.
use horizon_core::geometry::Vec3;

/// `throwHalfStack`: a stack is thrown when it is over half of `max_stack`, or over 24.
pub fn is_thrown(count: u8, max_stack: u32) -> bool {
    u32::from(count) > max_stack / 2 || count > 24
}

/// The amount thrown from a stack of `count`: its half when over half of `max_stack`, else what is over 24.
pub fn thrown_count(count: u8, max_stack: u32) -> u8 {
    if u32::from(count) > max_stack / 2 { count / 2 } else { count - 24 }
}

/// The velocity of a toss from `me` toward `at`: a vector of length `speed` along the direction (a zero distance counts as 1e-9).
pub fn toss_velocity(me: Vec3, at: Vec3, speed: f64) -> Vec3 {
    let d = Vec3::new(at.x - me.x, at.y - me.y, at.z - me.z);
    let len = (d.x * d.x + d.y * d.y + d.z * d.z).sqrt().max(1e-9);
    Vec3::new(d.x / len * speed, d.y / len * speed, d.z / len * speed)
}
