//! The gift of a villager to a hero: where it falls, a block above the villager.
use horizon_core::geometry::Vec3;

/// The spot a gift falls from: one block above the villager's position.
pub fn gift_spot(p: Vec3) -> Vec3 {
    Vec3::new(p.x, p.y + 1.0, p.z)
}
