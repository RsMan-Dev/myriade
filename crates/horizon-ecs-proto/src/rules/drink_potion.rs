//! The witch's drinks: the far-target test, a roll against a chance, the speed under a drink and the healing.

/// A target farther than 11 blocks, from the offset to it (the squared distance is above 121).
pub fn beyond_11_blocks(dx: f64, dy: f64, dz: f64) -> bool {
    dx * dx + dy * dy + dz * dz > 121.0
}

/// A roll of the creature's stream lands under the chance `p`.
pub fn roll_hits(draw: f64, p: f64) -> bool {
    draw < p
}

/// The speed while drinking: the penalty taken off, never under zero.
pub fn slowed(speed: f64, penalty: f64) -> f64 {
    (speed - penalty).max(0.0)
}

/// The health after a healing drink, capped at the species' maximum.
pub fn healed(health: f32, heal: f32, max: f32) -> f32 {
    (health + heal).min(max)
}
