//! The stacks of the villager's inventory: how many of an item a slot takes, and how many a new slot holds.

/// The size of a stack (`Item.getMaxStackSize`) for the food the villager carries.
pub const STACK: u8 = 64;

/// How many items a stack of `have` takes from `count` more: up to the stack's room.
pub fn fill(have: u8, count: u8) -> u8 {
    (STACK - have).min(count)
}

/// How many items a new slot holds out of `count` left: up to a stack.
pub fn put(count: u8) -> u8 {
    count.min(STACK)
}
