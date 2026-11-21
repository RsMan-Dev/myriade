//! The villager's hiding: the reach of a home, the end of the hiding after the bell, and the arrival at the hiding place.

/// The squared reach of a home that is close: `close` blocks.
pub fn home_reach_sq(close: i32) -> f64 {
    f64::from(close * close)
}
/// The hiding is over: 300 ticks after the bell `heard`.
pub fn hiding_over(heard: i64, now: i64) -> bool {
    heard + 300 <= now
}
/// The villager has arrived at its hiding place: its squared distance to the place is under 3 blocks.
pub fn arrived_at_place(dist_sq: f64) -> bool {
    dist_sq < 9.0
}
