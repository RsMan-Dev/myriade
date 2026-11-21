//! `FollowTemptation` (`brain/behaviours/follow_temptation.rs`): the close distance of the walk to the tempting player, the cooldown left
//! when the temptation ends, and whether a player is near enough to stop. The walk's speed is `rules::follow_adult::walk_speed`.
pub use crate::rules::follow_adult::walk_speed;

/// The walk's close-enough distance in blocks (the `Mob`'s `WalkTarget`).
pub const WALK_CLOSE: i32 = 2;
/// The cooldown left when it stops (`temptation_cooldown_ticks.set(100)`).
pub const COOLDOWN_TICKS: i64 = 100;

/// Whether a player at squared distance `dist2` is within `close` blocks: the walk then stops.
pub fn is_near(dist2: f64, close: f64) -> bool {
    dist2 < close * close
}
