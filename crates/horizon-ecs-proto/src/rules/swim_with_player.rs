//! The dolphin's escort of a swimming player: the start, the continuation and the stop of the walk, as decisions on distances.

/// The decision of `canUse`, from its parts: a swimming player within range that the animal does not target.
pub fn may_start(swimming: Option<bool>, d2: f64, range: f64, targets_player: bool) -> bool {
    swimming == Some(true) && d2 <= range * range && !targets_player
}
/// The decision of `canContinue`: the player still swims and is within `keep_sqr`.
pub fn may_continue(swimming: Option<bool>, d2: f64, keep_sqr: f64) -> bool {
    swimming == Some(true) && d2 < keep_sqr
}
/// The walk stops within `near_sqr` (`tick`: `distanceToSqr < 6.25`).
pub fn is_close(d2: f64, near_sqr: f64) -> bool {
    d2 < near_sqr
}
