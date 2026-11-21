//! Calculations of the puffed fish's size (`PufferfishPuff`): the scale of each of its three sizes, and the body it gives.

/// The scale of a size: 0 is half, 1 is 0.7, 2 is full.
pub fn puff_scale(size: u8) -> f32 {
    match size {
        0 => 0.5,
        1 => 0.7,
        _ => 1.0,
    }
}
/// The body at a scale: width, height and eye height of the base, scaled (the eye in double precision).
pub fn puffed_body(base: (f32, f32, f64), scale: f32) -> (f32, f32, f64) {
    (base.0 * scale, base.1 * scale, base.2 * f64::from(scale))
}
