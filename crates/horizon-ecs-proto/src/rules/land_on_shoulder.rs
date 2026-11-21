//! The landing on a shoulder: the wait of the age's deadline, from the age and the age that must pass.

/// The ask armed when the creature is `age` ticks old and must be `need` ticks old: the ticks left, counted from now.
pub fn arm_wait(age: u64, need: u64) -> i32 {
    (need + 1 - age) as i32
}
