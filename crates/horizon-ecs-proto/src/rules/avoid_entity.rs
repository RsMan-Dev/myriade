//! Calculations of the avoid goal: its reach from a point, and the speed it runs at.
use horizon_core::geometry::Vec3;

/// `AvoidEntityGoal` reach: `to` is within `max_dist` blocks of `from` (at least 2 blocks, squared distance) and within
/// `max_dist + 3` blocks of it in height.
pub fn in_reach(max_dist: f64, from: Vec3, to: Vec3) -> bool {
    let r = max_dist.max(2.0);
    let dist2 = (from.x - to.x).powi(2) + (from.y - to.y).powi(2) + (from.z - to.z).powi(2);
    (to.y - from.y).abs() <= max_dist + 3.0 && dist2 <= r * r
}
/// The speed of the run: `sprint` while the target is within 7 blocks (squared distance under 49), `walk` beyond.
pub fn run_speed(sprint: f64, walk: f64, dist2: f64) -> f64 {
    if dist2 < 49.0 { sprint } else { walk }
}
