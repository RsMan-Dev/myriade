//! Calculations of the garden raid (`RaidGarden`): the ticks of a wait of asks (two per ask, the first one counted), and
//! whether a carrot is ripe enough to take.

/// The carrot's age at which it is ripe (its maximum).
pub const CARROT_MAX_AGE: i32 = 7;

/// The ticks until the next ask of a wait of `asks` asks: two ticks each (the selector's phase), counting the first.
pub fn asks_ticks(asks: i32) -> i32 {
    2 * (asks.max(0) + 1)
}
/// A carrot is a target when its age, read from the block, is at its maximum.
pub fn ripe_carrot(age: Option<i32>) -> bool {
    age.is_some_and(|a| a >= CARROT_MAX_AGE)
}
