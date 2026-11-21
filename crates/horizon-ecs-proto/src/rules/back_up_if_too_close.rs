//! Calculations of the back-up goal: the target that makes the animal back up.

/// `too_close`: the target is in sight and closer than `range` blocks (`dist2`, the squared distance to its feet).
pub fn too_close(range: f64, seen: bool, dist2: f64) -> bool {
    seen && dist2 < range * range
}
