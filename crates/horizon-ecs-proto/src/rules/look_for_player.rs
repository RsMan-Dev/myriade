//! The gaze test of a player on the goal's creature (`look_for_player`): does the player's look point at the eye, within a cone.

/// Whether a player looking along `look` faces the point at `dir` from him, within the cone of 0.025 at that distance.
/// A point at the very player (`dir` near zero) is never faced.
pub fn gaze_hits(look: [f64; 3], dir: [f64; 3]) -> bool {
    let dist = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    if dist < 1.0e-6 {
        return false;
    }
    let dot = (look[0] * dir[0] + look[1] * dir[1] + look[2] * dir[2]) / dist;
    dot > 1.0 - 0.025 / dist
}
