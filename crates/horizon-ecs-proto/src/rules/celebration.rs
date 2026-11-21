//! Celebration of a raider after a lost raid (`Raider.RaiderCelebration`): the odds of its cheer and of its jump, and when it starts.

/// A cheer: one draw in this many of the goal's ticks.
pub const CHEER_ODDS: i32 = 50;
/// A jump: one draw in this many of the goal's ticks.
pub const JUMP_ODDS: i32 = 25;

/// A draw hit the odds (the draw is `next_int(odds)`, a hit is 0).
pub fn hit(draw: i32) -> bool {
    draw == 0
}

/// The raider starts celebrating: it is alive, it has no target, it belongs to a raid, and the raid is lost.
pub fn can_celebrate(alive: bool, has_target: bool, in_raid: bool, lost: bool) -> bool {
    alive && !has_target && in_raid && lost
}
