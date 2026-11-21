//! Calculations of the thrown item (`PlayWithItems`, `mobs/...` throw): the launch velocity from the yaw and pitch, the push,
//! the lift and the random spread.
use horizon_core::jmath;

/// The launch velocity `[x, y, z]`: the push along the yaw and pitch (radians), the lift upward, and the spread `pow2`
/// in the random direction `dir` (radians).
pub fn throw_velocity(push: f64, lift: f64, yaw: f64, pitch: f64, dir: f64, pow2: f64) -> [f64; 3] {
    [
        push * -jmath::sin(yaw) * jmath::cos(pitch) + jmath::cos(dir) * pow2,
        push * jmath::sin(pitch) * lift,
        push * jmath::cos(yaw) * jmath::cos(pitch) + jmath::sin(dir) * pow2,
    ]
}
