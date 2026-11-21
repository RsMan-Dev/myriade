//! `FollowAdult` (`brain/behaviours/follow_adult.rs`): the range a baby follows an adult or a player at, and the walk's speed.

/// Whether a target at squared distance `dist2` is in the range: `min` included, `max` excluded (`closerThan(17)`, not `closerThan(3)`).
pub fn in_follow_range(dist2: f64, min: f64, max: f64) -> bool {
    dist2 >= min * min && dist2 < max * max
}

/// The walk's speed: the speed in water, else the species' dry speed when it has one (`speed` otherwise).
pub fn walk_speed(wet: bool, speed: f32, dry: Option<f32>) -> f32 {
    if wet { speed } else { dry.unwrap_or(speed) }
}
