//! `LookAtPlayer` (`brain/behaviours/look_at_player.rs`): the reach of the player to look at.

/// Whether a player at squared distance `dist2` can be chosen: 36 squared or less.
pub fn in_reach(dist2: f64) -> bool {
    dist2 <= 36.0
}
