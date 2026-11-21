//! Calculations of the ranged bow (`RangedBowAttackGoal`): the bow's power from the pull, and the yaw wrap toward a target.

/// `BowItem.getPowerForTime`: the power of a pull of `ticks` ticks, at most 1.
pub fn bow_power(ticks: i32) -> f32 {
    let t = ticks as f32 / 20.0;
    ((t * t + t * 2.0) / 3.0).min(1.0)
}
/// Wraps an angle in degrees to [-180, 180), keeping the sign of the remainder of `a` (the aim's own wrap).
pub fn wrap_degrees(a: f32) -> f32 {
    let mut a = a % 360.0;
    if a >= 180.0 {
        a -= 360.0;
    }
    if a < -180.0 {
        a += 360.0;
    }
    a
}
