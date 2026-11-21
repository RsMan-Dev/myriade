//! Draw of a baby's bed session (`JumpOnBed`, `villager_brain.rs:1553-1580`).

/// The jumps of a session: 3 to 6, drawn by `draw(n)`, which gives an integer in `0..n`.
pub fn session_jumps(draw: &mut dyn FnMut(i32) -> i32) -> i32 {
    3 + draw(4)
}
