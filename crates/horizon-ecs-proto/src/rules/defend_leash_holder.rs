//! A leashed creature's defence: the holder's attacker is beyond the follow range or not.

/// The squared distance `d2` is beyond the squared range.
pub fn beyond_range(d2: f64, range: f64) -> bool {
    d2 > range * range
}
