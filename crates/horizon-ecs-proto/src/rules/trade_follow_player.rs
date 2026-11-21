//! `TradeFollowPlayer` (`brain/behaviours/trade_follow_player.rs`): the reach of the player to follow, and the walk's pace.

/// The walk's speed towards the player.
pub const SPEED: f32 = 0.5;
/// The walk's close-enough distance in blocks.
pub const CLOSE: i32 = 2;

/// Whether a player at squared distance `dist2` is followed: 16 squared or less.
pub fn in_reach(dist2: f64) -> bool {
    dist2 <= 16.0
}
