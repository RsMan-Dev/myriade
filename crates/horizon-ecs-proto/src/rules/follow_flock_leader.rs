//! The school's timing and reach: the delay of a search, the ticks of a deadline, the asks left, and the leash of a follower.

/// `Goal.reducedTickDelay`: half the delay, rounded up.
pub fn reduced(n: i32) -> i32 {
    (n + 1) / 2
}

/// The delay of a search from its draw `next_int(200)`: `reduced(200 + draw % 20)`.
pub fn search_delay(draw: i32) -> i32 {
    reduced(200 + draw % 20)
}

/// The ticks of a deadline `asks` asks away: a selector asks every 2 ticks.
pub fn ask_ticks(asks: i32) -> u64 {
    2 * (asks.max(0) as u64 + 1)
}

/// The asks left of a deadline armed at `armed_at`, read at `now` (`armed_at > now`).
pub fn asks_left(armed_at: u64, now: u64) -> i32 {
    (((armed_at - now) / 2).saturating_sub(1)) as i32
}

/// `canBeFollowed`: it has followers and room for more.
pub fn can_be_followed(max: i32, followers: i32) -> bool {
    followers > 0 && followers + 1 < max
}

/// The follower loses its leader beyond 11 blocks (squared distance above 121).
pub fn beyond_leash(d2: f64) -> bool {
    d2 > 121.0
}
