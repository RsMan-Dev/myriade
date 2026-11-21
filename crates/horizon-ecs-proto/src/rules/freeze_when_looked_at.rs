//! The enderman's stare: the range test on the player, and the cone of the player's gaze around the line to the enderman.

/// The player is out of range: the squared distance `dx² + dy² + dz²` is above `range_sqr`.
pub fn beyond_range(dx: f64, dy: f64, dz: f64, range_sqr: f64) -> bool {
    dx.powi(2) + dy.powi(2) + dz.powi(2) > range_sqr
}

/// The gaze `look` (unit) points within the cone of `gaze_k / distance` around `dir` (the line to the enderman).
/// No gaze when the enderman is at the eye (distance under 1e-6).
pub fn in_cone(dir: [f64; 3], look: [f64; 3], gaze_k: f64) -> bool {
    let dist = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    if dist < 1.0e-6 {
        return false;
    }
    let dot = (look[0] * dir[0] + look[1] * dir[1] + look[2] * dir[2]) / dist;
    dot > 1.0 - gaze_k / dist
}
