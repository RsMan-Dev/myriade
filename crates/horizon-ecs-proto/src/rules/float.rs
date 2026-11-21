//! The float of an animal in a fluid: its buoyancy threshold, whether the fluid is deep enough, and the jump rolled each tick.

/// `Mob.getFluidJumpThreshold`: the depth of fluid from which the animal floats; an eye height under 0.4 gives 0.
pub fn buoyancy(eye_height: f64) -> f64 {
    if eye_height < 0.4 { 0.0 } else { 0.4 }
}

/// The fluid is deeper than the threshold, or it is lava.
pub fn is_deep(water: f64, threshold: f64, in_lava: bool) -> bool {
    water > threshold || in_lava
}

/// `FloatGoal.tick`: a jump in 80 % of the ticks, from one draw of the creature's stream.
pub fn jumps(draw: u32) -> bool {
    f64::from(draw >> 8) / f64::from(1u32 << 24) < 0.8
}
