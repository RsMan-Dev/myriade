//! The llama's strength against the wolf's draw: the test of the avoid goal's target roll.

/// `Wolf.WolfAvoidEntityGoal.canUse`: a strength `s` passes the uniform draw `u` in [0, 1) when `u < (s + 1) / 5`.
pub fn strength_passes(strength: i32, u: f64) -> bool {
    u < f64::from(strength + 1) / 5.0
}
