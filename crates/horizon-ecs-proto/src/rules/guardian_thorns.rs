//! The guardian's spikes: the thorns a blow draws, and when a blow draws them.

/// The thorns a blow draws from a still guardian: 2 damage (`Guardian.hurtServer`).
pub const THORNS: f32 = 2.0;

/// The spikes strike: the guardian is not moving, the blow is direct, and its cause is an entity (not a projectile, not `#avoids_guardian_thorns`).
pub fn spikes_strike(moving: bool, direct: bool, by_entity: bool) -> bool {
    !moving && direct && by_entity
}
