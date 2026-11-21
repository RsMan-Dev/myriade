//! The wings of a hen (`Chicken.aiStep`): one step of the flap and the rest of its decay (`flapping` is geometric).

/// One step of the wings: `speed` closes on 0 or 1, `flapping` restarts at 1 in the air and decays by 0.9, and `flap` turns by twice it.
pub fn wing_step(speed: f32, flapping: f32, flap: f32, on_ground: bool) -> (f32, f32, f32) {
    let speed = (speed + if on_ground { -1.0 } else { 4.0 } * 0.3).clamp(0.0, 1.0);
    let mut flapping = if !on_ground && flapping < 1.0 { 1.0 } else { flapping };
    flapping *= 0.9;
    (speed, flapping, flap + flapping * 2.0)
}

/// The flap a hen's wings add when they go to rest: the sum of the decay still to come, `flapping * 0.9 * 2 / (1 - 0.9)`.
pub fn wing_settle(flap: f32, flapping: f32) -> f32 {
    flap + flapping * 0.9 * 2.0 / (1.0 - 0.9)
}
