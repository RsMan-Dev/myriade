//! The rolls of playing dead: the first roll is `nextInt(3) == 0`, the second holds when the draw is under the damage or the health was under half.

/// The first roll of `Axolotl.hurtServer`: the draw `nextInt(3)` is 0.
pub fn is_first_roll(draw: i32) -> bool {
    draw == 0
}

/// The second roll: the draw `nextInt(3)` is under the damage `amount`, or the health `before` was under half of `max`.
pub fn hurt_enough(draw: i32, amount: f32, before: f32, max: f32) -> bool {
    (draw as f32) < amount || before / max < 0.5
}
