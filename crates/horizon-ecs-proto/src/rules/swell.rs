//! The swell's reading of its target: close enough to start, and the direction read each tick.

/// Whether a target at squared distance `dist_sqr` is under 3 blocks: the swell starts.
pub fn is_close(dist_sqr: f64) -> bool {
    dist_sqr < 9.0
}

/// The swell direction read each tick: +1 when the target is seen within 7 blocks (squared distance at most 49), else -1.
pub fn swell_dir(dist_sqr: f64, seen: bool) -> i8 {
    if dist_sqr <= 49.0 && seen { 1 } else { -1 }
}
