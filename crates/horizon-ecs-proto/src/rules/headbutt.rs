//! Throw of a hoglin's headbutt (`HoglinBase.throwTarget`, `mobs/piglin.rs:2061-2090`): away from the attacker and up.
use horizon_core::geometry::Vec3;

/// The velocity given to the struck creature: `away` is its offset from the attacker (x, z), `knockback` the attacker's, and
/// `u` three uniform draws in `[0, 1)` for the angle, the scale and the lift, in that order.
pub fn throw_velocity(away: Vec3, knockback: f64, u: [f64; 3]) -> Vec3 {
    let angle = ((u[0] * 21.0).floor().min(20.0) - 10.0).to_radians();
    let scale = knockback * (u[1] * 0.5 + 0.2);
    let up = knockback * u[2] * 0.5;
    let push = away.normalize().scale(scale).rotate_y(angle as f32);
    Vec3::new(push.x, up, push.z)
}
