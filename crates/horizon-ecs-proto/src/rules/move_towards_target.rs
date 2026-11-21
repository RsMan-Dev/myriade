//! Reach tests of the approach toward a target (`MoveTowardsTargetGoal`): the squared distance against the squared range, the
//! target is refused beyond it and kept within it (`within` of `Params`).

/// True when the squared distance `dist2` is beyond the range `within` (strictly farther than `within` blocks).
pub fn beyond_within(dist2: f64, within: f64) -> bool {
    dist2 > within * within
}
/// True when the squared distance `dist2` is inside the range `within` (strictly nearer than `within` blocks).
pub fn within_range(dist2: f64, within: f64) -> bool {
    dist2 < within * within
}
