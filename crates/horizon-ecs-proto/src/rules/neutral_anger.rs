//! Calculations of the persistent anger of a neutral mob (`NeutralMob`, `polar_bear.rs:172-196`): the length of the countdown
//! after the target is lost, and the ticks left until its end.

/// The countdown after the target is lost: `base` ticks plus a draw below `spread` (`draw(bound)` is the entity's stream).
pub fn anger_countdown(base: i32, spread: i32, draw: impl FnOnce(i32) -> i32) -> i32 {
    base + draw(spread)
}
/// The ticks left before `at` for an entity whose own time is `now`, capped at the largest wheel delay (`i32::MAX`).
pub fn ticks_until(at: u64, now: u64) -> i32 {
    at.saturating_sub(now).min(i32::MAX as u64) as i32
}
