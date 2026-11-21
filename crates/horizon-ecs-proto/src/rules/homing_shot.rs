//! Values of a homing volley (`homing_shot`): the wait to the next shot and the pitch of its sound.

/// The wait to the next shot: `base` plus `step` times the draw `n` (in `0..count`).
pub fn next_wait(base: i32, step: i32, n: i32) -> i32 {
    base + step * n
}

/// The pitch of the sound: the difference of two uniform draws, spread, around 1.
pub fn pitch_of(a: f64, b: f64, spread: f32) -> f32 {
    (a - b) as f32 * spread + 1.0
}
