//! Physics of a sulfur cube holding a block (`sulfur_cube.rs` 297-335): the horizontal drag of its ground or air state, and
//! the bounce a landing gives back. Raw multipliers: the vanilla 0.91 and 0.6 are not applied here.

/// The horizontal multiplier of the speed: the friction on the ground, the air drag in the air.
pub fn drag_factor(on_ground: bool, friction: f32, air_drag: f32) -> f64 {
    f64::from(if on_ground { friction } else { air_drag })
}

/// A landing gives back the bounce: it is taken on the first tick on the ground after a fall, with a falling speed.
pub fn bounces(on_ground: bool, was_airborne: bool, vy: f64) -> bool {
    on_ground && was_airborne && vy < 0.0
}

/// The upward speed after the bounce: the falling speed `vy` reversed and scaled by the bounciness.
pub fn bounce_vy(vy: f64, bounciness: f32) -> f64 {
    -vy * f64::from(bounciness)
}
