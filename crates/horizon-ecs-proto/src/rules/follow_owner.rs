//! The pet's follow: the tests on the squared distance to its owner, and the ticks between two path asks.
/// The owner is beyond the start radius: `d2 >= start * start`.
pub fn beyond_start(d2: f64, start: f64) -> bool {
    d2 >= start * start
}

/// The owner is beyond the stop radius: `d2 > stop * stop`.
pub fn beyond_stop(d2: f64, stop: f64) -> bool {
    d2 > stop * stop
}

/// The owner is far enough to teleport to: 12 blocks, squared.
pub fn teleports(d2: f64) -> bool {
    d2 >= 144.0
}

/// The ticks between two path asks: `(10 + 1) / 2`.
pub fn repath_ticks() -> i32 {
    (10 + 1) / 2
}
