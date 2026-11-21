//! The food reserve: its thresholds, its total, and the digestion of the eaten part.

/// The level under which a villager wants more food.
pub const WANT: i32 = 12;
/// The level from which a villager has food to spare.
pub const EXCESS: i32 = 24;
/// The points one digestion takes off the eaten food.
pub const DIGEST: i32 = 12;

/// The total level: the eaten, undigested points plus the inventory's points.
pub fn total(eaten: i32, stock: i32) -> i32 {
    eaten + stock
}

/// The level is enough to breed.
pub fn can_breed(level: i32) -> bool {
    level >= WANT
}

/// The level is under the want.
pub fn wants_more(level: i32) -> bool {
    level < WANT
}

/// The level is in excess.
pub fn has_excess(level: i32) -> bool {
    level >= EXCESS
}

/// The eaten food after one digestion, never under zero.
pub fn digested(food: i32) -> i32 {
    (food - DIGEST).max(0)
}
