//! The turtle's search for water: the countdown drawn at each search, and the wait before the next one.

/// The countdown drawn at a search, halved and rounded up: `reduced(n)`.
pub fn reduced(n: i32) -> i32 {
    (n + 1) / 2
}

/// The wait before the next search, in ticks: `2 * (countdown + 1)`.
pub fn search_wait(countdown: i32) -> i32 {
    2 * (countdown + 1)
}
