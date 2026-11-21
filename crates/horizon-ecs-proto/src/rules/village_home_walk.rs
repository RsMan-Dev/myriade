//! `VillageHomeWalk` (`brain/behaviours/village_home_walk.rs`): the walk's speed and the distance under which the villager is home.

/// The walk's speed towards the home.
pub const SPEED: f32 = 0.5;

/// Whether the villager at squared distance `dist2` from its home is already there: 4 squared or less.
pub fn at_home(dist2: f64) -> bool {
    dist2 <= 4.0
}
