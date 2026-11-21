//! Calculations of the animal's conversion count (`PiglinAi` zombification, `mobs/piglin.rs:1964-2000`): when the count is
//! due and when it started.

/// The conversion is due at the tick the count ends: `after` ticks from `now`, one of them already counted.
pub fn until(now: u64, after: u64) -> u64 {
    now + after - 1
}

/// The count is over at `now`.
pub fn count_over(now: u64, until: u64) -> bool {
    now >= until
}
