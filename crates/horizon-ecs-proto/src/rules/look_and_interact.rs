//! `LookAndInteract` (`brain/behaviours/look_and_interact.rs`): the reach of the player to look at and interact with.

/// Whether a player at squared distance `dist2` can be chosen: 16 squared or less.
pub fn in_reach(dist2: f64) -> bool {
    dist2 <= 16.0
}
