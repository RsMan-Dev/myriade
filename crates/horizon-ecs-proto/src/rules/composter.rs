//! Calculations of the villager's composter work (`WorkAtComposter`, `villager_brain.rs:1636-1660`): the check's spacing, and
//! the reach of the job site.

/// Two checks of the work are at least this many ticks apart.
pub const CHECK_EVERY: i64 = 300;
/// The villager works at its site within this distance of the block's centre, in blocks.
pub const REACH: f64 = 1.73;

/// The work is checked again: `CHECK_EVERY` ticks since the last check.
pub fn check_due(now: u64, last: u64) -> bool {
    (now as i64) - (last as i64) >= CHECK_EVERY
}

/// The villager is at its site: its squared distance `dist_sq` to the block's centre is under `REACH` squared.
pub fn at_site(dist_sq: f64) -> bool {
    dist_sq < REACH * REACH
}
