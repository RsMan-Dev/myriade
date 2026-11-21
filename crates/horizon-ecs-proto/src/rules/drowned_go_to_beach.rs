//! The drowned's search for a beach: the countdown of a search, the wait to the next one, and the depth test.

/// `reduced(n)`: half of `n`, rounded up.
pub fn reduced(n: i32) -> i32 {
    (n + 1) / 2
}

/// The countdown of a search: `reduced(200 + draw)` for the draw of `nextInt(200)`.
pub fn countdown(draw: i32) -> i32 {
    reduced(200 + draw)
}

/// The ticks until the next search, after a countdown.
pub fn next_search(countdown: i32) -> i32 {
    2 * (countdown + 1)
}

/// The sea surface is at most 3 blocks above the drowned: its height is at least `sea - 3`.
pub fn sea_close(y: f64, sea: i32) -> bool {
    y >= f64::from(sea - 3)
}
