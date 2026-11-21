//! Calculations of the golem's flower offer (`IronGolem.OfferFlowerGoal`): the wait of the draw, the reach and height of a
//! candidate, and the count of ticks the offer is held.

/// The reach of a candidate, in blocks (the box of the search, and its squared distance).
pub const REACH: f64 = 6.0;
/// The height a candidate may differ from the golem by, in blocks.
pub const HEIGHT: f64 = 2.0;

/// The wait of an ask: twice a geometric draw of `chance` (`draw(bound)` is the entity's stream).
pub fn offer_wait(draw: impl FnOnce(i32) -> i32, chance: i32) -> i32 {
    2 * draw(chance)
}
/// True when the squared distance `d2` of a candidate is within `REACH`.
pub fn in_offer_reach(d2: f64) -> bool {
    d2 <= REACH * REACH
}
/// True when a candidate's height difference `dy` is beyond `HEIGHT`.
pub fn beyond_offer_height(dy: f64) -> bool {
    dy.abs() > HEIGHT
}
/// The ticks the offer is held: one more than the configured `ticks`, the tick that starts the goal also runs it once.
pub fn hold_ticks(ticks: i32) -> i32 {
    ticks + 1
}
