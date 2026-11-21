//! The target's clocks: a target is tired when its last reach is older than `tired_ticks`, stale when its last attackable look is older than `window`.

/// True when the walk toward the target was unreachable since `since` for more than `tired_ticks` (strictly).
pub fn is_tired(now: i64, since: i64, tired_ticks: i64) -> bool {
    now - since > tired_ticks
}

/// True when the target was last attackable at `last` and that is more than `window` ticks before `now`.
pub fn is_stale(now: u64, last: u64, window: i64) -> bool {
    now.saturating_sub(last) as i64 > window
}
