//! The follower's walk: the reach of its scan, the next path ask, and the step back from an animal too close.

/// The reach of the scan of an animal of width `width`: the scan area, the half width and one block.
pub fn scan_reach(scan_area: f64, width: f64) -> f64 {
    scan_area + width / 2.0 + 1.0
}

/// The recalc after a path ask: `reduced(recalc)`.
pub fn recalc_after(recalc: i32) -> i32 {
    (recalc + 1) / 2
}

/// One coordinate of the step back: `from - (toward - from)`, away from the animal.
pub fn step_back(from: f64, toward: f64) -> f64 {
    from - (toward - from)
}
