//! Calculations of the push by a player (`PushOnTouch`): the knockback left after the resistance, the push's speed, and the
//! push vector with its lift on the ground.

/// The share of the knockback left: one minus the resistance (clamped to -2..1), not under zero.
pub fn knockback_factor(resistance: f64) -> f64 {
    (1.0 - resistance.clamp(-2.0, 1.0)).max(0.0)
}
/// The push's speed: twice the player's speed times the speed factor, clamped to 0..max.
pub fn push_speed(player_speed: f64, speed_factor: f64, speed_max: f64) -> f64 {
    (player_speed * 2.0 * speed_factor).clamp(0.0, speed_max)
}
/// The push vector `[x, y, z]` away from the player at `(dx, dz)`: the horizontal part along that direction (its length
/// at least 1e-4), and a lift on the ground.
pub fn push_vector(dx: f64, dz: f64, kb: f64, speed: f64, lift: f64, on_ground: bool) -> [f64; 3] {
    let len = (dx * dx + dz * dz).sqrt().max(1.0e-4);
    [dx / len * kb * speed, if on_ground { kb * lift * speed } else { 0.0 }, dz / len * kb * speed]
}
