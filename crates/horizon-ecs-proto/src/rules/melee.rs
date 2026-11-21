//! The melee goal: the grid of its checks, which asks fall off it.

/// A check of `canUse` falls off the grid of the creature's phase: after the first tick, on the odd sums of tick and phase.
pub fn off_grid(now: u64, phase: u64) -> bool {
    now > 1 && (now + phase) % 2 == 1
}
