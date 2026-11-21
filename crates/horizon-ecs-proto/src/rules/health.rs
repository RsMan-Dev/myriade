//! The numbers of a sonic boom: its damage and the push it gives along its direction, scaled by the target's knockback resistance.

/// The damage of a boom, whatever the armour.
pub const BOOM_DAMAGE: f32 = 10.0;

/// The push of a boom: 2.5 horizontally and 0.5 vertically, both scaled by `1 - resistance`.
pub fn boom_push(direction: [f64; 3], resistance: f64) -> [f64; 3] {
    let (horizontal, vertical) = (2.5 * (1.0 - resistance), 0.5 * (1.0 - resistance));
    [direction[0] * horizontal, direction[1] * vertical, direction[2] * horizontal]
}
