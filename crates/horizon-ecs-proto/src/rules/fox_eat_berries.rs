//! The berry search and pick: the next search delay, the ticks of a deadline, and the berries of a sweet bush.

/// `Goal.reducedTickDelay`: half the delay, rounded up.
pub fn reduced(n: i32) -> i32 {
    (n + 1) / 2
}

/// The delay of a search from its draw `next_int(200)`: `reduced(200 + draw)`.
pub fn search_delay(draw: i32) -> i32 {
    reduced(200 + draw)
}

/// The ticks of a deadline `next` asks away: two ticks per ask.
pub fn ask_ticks(next: i32) -> i32 {
    2 * (next + 1)
}

/// The berries of a sweet bush pick: `1 + U(2)`, one more at age 3, from the draw `next_int(2)`.
pub fn pick_count(draw: i32, age: i32) -> i32 {
    1 + draw + i32::from(age == 3)
}
