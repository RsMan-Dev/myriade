//! `MakeLove` (`brain/behaviours/make_love.rs`): the distance at which two mates meet.

/// Whether two mates at squared distance `dist2` meet: less than 9 (3 blocks).
pub fn meet(dist2: f64) -> bool {
    dist2 < 9.0
}
