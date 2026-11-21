//! Calculations of the villagers' love: the delay of a birth, and the distance at which the pair is not yet close enough.

/// The delay of a birth planned at the start: `275 + draw` ticks, for the draw `draw` of `nextInt(50)`.
pub fn birth_delay(draw: i32) -> u64 {
    275 + draw as u64
}
/// The pair is still too far apart to give birth (`dist2`, squared): both keep walking together.
pub fn too_far_to_mate(dist2: f64) -> bool {
    dist2 > 5.0
}
