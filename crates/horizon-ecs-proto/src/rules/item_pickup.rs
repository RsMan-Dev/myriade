//! The dolphin's looting: how many items of a stack the hand takes.

/// The count taken from a stack of `count`: the whole stack, or one item.
pub fn take_count(count: u32, whole_stack: bool) -> u32 {
    if whole_stack { count } else { 1 }
}
