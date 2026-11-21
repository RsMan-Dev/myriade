//! The job-site strolls: the reach of a job site and the next try after a walk.

/// The site is out of reach: its squared distance is at or beyond `reach_sq`.
pub fn too_far(dist_sq: f64, reach_sq: f64) -> bool {
    dist_sq >= reach_sq
}
/// The next try of a stroll, `gap` ticks after `now`.
pub fn next_try(now: u64, gap: u64) -> u64 {
    now + gap
}
