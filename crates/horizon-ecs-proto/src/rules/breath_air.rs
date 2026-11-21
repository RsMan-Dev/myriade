//! Calculations of the air search: the wait before the goal is asked again.

/// The wait before the next air check, from the air now: `None` when the air is already under `threshold` (due now); else the
/// crossing of the threshold while the eyes are under (the air falls by one a tick), or 20 ticks while they are out.
pub fn air_wait(air: i32, threshold: i32, eyes_under: bool) -> Option<i32> {
    if air < threshold {
        return None;
    }
    Some(if eyes_under { (air - threshold).max(1) } else { 20 })
}
