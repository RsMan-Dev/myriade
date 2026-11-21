//! Calculations of the baby's follow: the band of distances that starts it, the distance that keeps it, the walk's reach, and
//! the slow poll while no adult is in range.
/// The slow poll of a baby with no adult in range, in ticks (the crossing of the range inside one column).
pub const WAIT: i32 = 20;

/// `canUse` band: the adult is at least `min` blocks away and less than `max + 1` blocks away (squared distances `dist2`).
pub fn starts_at(dist2: f64, min: f64, max: f64) -> bool {
    dist2 >= min * min && dist2 < (max + 1.0) * (max + 1.0)
}
/// `canContinueToUse`: the adult is farther than `min - 1` blocks (squared distance `dist2`).
pub fn keeps_following(dist2: f64, min: f64) -> bool {
    dist2 > (min - 1.0).powi(2)
}
/// The reach of the walk toward the adult: `min - 1` blocks, as a whole number.
pub fn walk_reach(min: f64) -> i32 {
    min as i32 - 1
}
