//! Values of `GoAndGiveItemsToTarget` (`give_to_owner`): the watch radius, the path countdown and the owner's range.

/// The radius the owner is watched at: the range, capped at 64 blocks.
pub fn watch_radius(range: f64) -> f32 {
    range.min(64.0) as f32
}

/// One tick of the path countdown: the new count, and whether a path is made to the owner now (every 10 ticks).
pub fn repath_tick(recalc: i32) -> (i32, bool) {
    let next = recalc - 1;
    if next <= 0 { (10, true) } else { (next, false) }
}

/// The owner is within `range` blocks, given the squared distance.
pub fn owner_in_range(dist_sqr: f64, range: f64) -> bool {
    dist_sqr <= range * range
}
