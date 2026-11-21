//! Flight decisions of a bat (`Bat.customServerAiStep`): the spot it flutters to, whether it is near it, the block it hangs
//! from, and the steering toward the spot.
use horizon_core::geometry::Vec3;

/// The spot is near when the bat is within 4 blocks of the centre of its block (squared distance under 4).
pub fn is_near(t: [i32; 3], x: f64, y: f64, z: f64) -> bool {
    let d = |a: i32, b: f64| f64::from(a) + 0.5 - b;
    d(t[0], x).powi(2) + d(t[1], y).powi(2) + d(t[2], z).powi(2) < 4.0
}

/// A new spot: 7 blocks around on x and z, 6 blocks down on y (the draws `draw(bound)` in that order: x, y, z).
pub fn spot(x: f64, y: f64, z: f64, mut draw: impl FnMut(i32) -> f64) -> [i32; 3] {
    let (x, y, z) = (x + draw(7) - draw(7), y + draw(6) - 2.0, z + draw(7) - draw(7));
    [x.floor() as i32, y.floor() as i32, z.floor() as i32]
}

/// The block above a bat's place: the one a conductor is looked at in.
pub fn above(x: f64, y: f64, z: f64) -> [i32; 3] {
    [x.floor() as i32, y.floor() as i32 + 1, z.floor() as i32]
}

/// The velocity toward the spot: a tenth of the gap to 0.5 / 0.7 / 0.5 of speed, from the velocity `u` and the gap `(dx, dy, dz)`.
pub fn steer(u: Vec3, dx: f64, dy: f64, dz: f64) -> Vec3 {
    let sign = |v: f64| if v == 0.0 || v.is_nan() { v } else { v.signum() };
    let tenth = f64::from(0.1_f32);
    Vec3::new(u.x + (sign(dx) * 0.5 - u.x) * tenth, u.y + (sign(dy) * f64::from(0.7_f32) - u.y) * tenth, u.z + (sign(dz) * 0.5 - u.z) * tenth)
}
