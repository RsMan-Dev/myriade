//! Admiration of the piglin: the tick its admiration ends (`mobs/piglins.rs:392-499`, `start_admiring`).

/// `start_admiring`: the tick the admiration ends, `duration` ticks after `now`.
pub fn until(now: u64, duration: i32) -> u64 {
    now + duration as u64
}
