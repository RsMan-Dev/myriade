//! Calculations shared by the panda goals (`panda.rs:44-62`, `105-120`, `186-199`): the asks until the first success of a
//! one-in-`n` ask. The ask that succeeds counts as one, hence the failures of the draw plus one.

/// Asks until the first success of a one-in-`n` ask: `draw(n)` gives the failures before it (the entity's stream), plus one.
pub fn asks_until_success(draw: impl FnOnce(i32) -> i32, n: i32) -> i32 {
    draw(n) + 1
}
