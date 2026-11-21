//! Calculations of the point-of-interest claim's retries (`AcquirePoi`, `poi_claim.rs`): the delay before the next try.

/// The next try after a failed claim: the delay grows by `jitter + 40` from the last delay (back to zero when the last
/// try was 400 or more ticks ago), at most 400. Returns the new delay and the tick it is due at.
pub fn retry_after(now: i64, previous: i64, delay: i32, jitter: i32) -> (i32, i64) {
    let base = if now - previous >= 400 { 0 } else { delay };
    let delay = (base + jitter + 40).min(400);
    (delay, now + i64::from(delay))
}
