//! Calculs du saut par bonds : attente avant le saut, durée de la direction, vitesse, puissance et poussée d'un bond.
use horizon_core::geometry::Vec3;

/// `RabbitMoveControl`: the wait after a landing, the short one when the path's speed modifier is `fast_from` or more.
pub fn path_wait(speed: f64, fast_from: f64, delay: i32, fast_delay: i32) -> i32 {
    if speed >= fast_from { fast_delay } else { delay }
}
/// `CubeMobMoveControl`: the wait on the ground, `(base + draw) * mul` ticks, divided by `div` while aggressive.
pub fn wander_wait(base: i32, draw: i32, mul: i32, aggressive: bool, div: i32) -> i32 {
    let pending = (base + draw) * mul.max(1);
    if aggressive { pending / div.max(1) } else { pending }
}
/// `CubeRandomDirection`: the ticks before the next draw of the direction, from the drawn `draw`.
pub fn redirect_ticks(base: i32, draw: i32) -> i32 {
    (base + draw + 1) / 2
}
/// The speed a hop goes at, as a modifier of the body's movement speed.
pub fn hop_speed(speed_modifier: f64, body_speed: f64) -> f32 {
    (speed_modifier * body_speed) as f32
}
/// The vertical speed of a bound: `high` against a wall, `low` up to `low_to`, `mid` above.
pub fn bound_power(wall: bool, speed: f64, low: f64, low_to: f64, mid: f64, high: f64) -> f64 {
    if wall {
        high
    } else if speed <= low_to {
        low
    } else {
        mid
    }
}
/// A bound's velocity: up to `power`, then a push forward and up when the body is almost still (`moveRelative(push, (0, 1.5, 1))`).
pub fn bound_velocity(v: Vec3, power: f64, push: f32, yaw: f32) -> Vec3 {
    let mut v = v;
    v.y = v.y.max(power);
    if v.x * v.x + v.z * v.z < 0.01 {
        let (s, c) = (f64::from((yaw * horizon_core::geometry::trig::DEG_TO_RAD).sin()), f64::from((yaw * horizon_core::geometry::trig::DEG_TO_RAD).cos()));
        // The input is normalised (0, 0.832, 0.555).
        let (f, up) = (f64::from(push) * 0.554_700_2, f64::from(push) * 0.832_050_3);
        v.x += -f * s;
        v.z += f * c;
        v.y += up;
    }
    v
}
