//! Calculations of the villager's bone meal (`UseBonemeal`, `villager_brain.rs:1713-1745`, tick `3044-3075`): the cycle's
//! tick, the session's gap, its cycle cap, the wait between two cycles, the ages a meal grows, and the reach of the crop.

/// A cycle is checked on the ticks that are multiples of this.
pub const CYCLE_EVERY: u64 = 10;
/// A session ends this many ticks after the last one, before a new one may start.
pub const SESSION_GAP: u64 = 160;
/// A session ends after this many cycles.
pub const MAX_CYCLES: i32 = 80;
/// The next cycle waits this many ticks after a meal.
pub const NEXT_CYCLE: u64 = 40;

/// A session may start: the last one ended `SESSION_GAP` ticks ago or never (`last` 0).
pub fn may_start(last: u64, now: u64) -> bool {
    !(last != 0 && last + SESSION_GAP > now)
}

/// The session is over after `worked` cycles.
pub fn session_full(worked: i32) -> bool {
    worked >= MAX_CYCLES
}

/// The next cycle's tick after a meal at `now`.
pub fn next_cycle_at(now: u64) -> u64 {
    now + NEXT_CYCLE
}

/// The ages a meal grows the crop by: the draw `r` of `nextInt(4)` plus 2 (2 to 5).
pub fn ages(r: i32) -> i32 {
    r + 2
}

/// The villager is within reach of the crop `q`: each of its blocks is within 1 of the crop's.
pub fn near_crop(here: [i32; 3], q: [i32; 3]) -> bool {
    (0..3).all(|i| (here[i] - q[i]).abs() <= 1)
}
