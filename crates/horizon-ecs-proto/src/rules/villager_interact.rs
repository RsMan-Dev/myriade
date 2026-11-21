//! `VillagerInteract` (`brain/behaviours/villager_interact.rs`): the reach of a villager to interact with, and the walk to it.

/// The walk's speed towards the villager.
pub const SPEED: f32 = 0.5;
/// The walk's close-enough distance in blocks.
pub const CLOSE: i32 = 2;

/// Whether a villager at squared distance `dist2` can be chosen: 64 squared or less.
pub fn in_reach(dist2: f64) -> bool {
    dist2 <= 64.0
}
