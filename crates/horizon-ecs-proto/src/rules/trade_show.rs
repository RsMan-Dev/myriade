//! The timing of the merchant's shown trades: the cycle of 40 ticks, the look timer, the range, and the next shown offer.

/// The ticks of one cycle: the shown result moves on each 40 ticks.
pub const CYCLE: i32 = 40;
/// The ticks a player's look at the trades lasts.
pub const LOOK_TICKS: u32 = 900;

/// Whether the cycle counter has reached a turn of the shown result.
pub fn cycle_done(cycle: u32) -> bool {
    cycle >= CYCLE as u32
}

/// The next shown offer among `len` (wraps to the first).
pub fn next_index(index: usize, len: usize) -> usize {
    (index + 1) % len
}

/// Whether the player at squared distance `d2` is close enough to be shown the trades (17 squared).
pub fn in_show_range(d2: f64) -> bool {
    d2 <= 17.0
}
