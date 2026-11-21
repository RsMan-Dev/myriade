//! The rules of the End crystals: their box, the feet of a spike's crystal, and whether a hit on one is taken.
use horizon_core::geometry::{Aabb, Vec3};

/// The crystal's box: 2 by 2, from its feet up 2 (`EndCrystal.getBoundingBox`).
pub fn bounds(p: Vec3) -> Aabb {
    Aabb::new(Vec3::new(p.x - 1.0, p.y, p.z - 1.0), Vec3::new(p.x + 1.0, p.y + 2.0, p.z + 1.0))
}

/// The feet of the crystal on a spike of centre `(cx, cz)` and `height`, given as f64 (the centre of its top block, one block above).
pub fn spike_crystal_at(cx: f64, height: f64, cz: f64) -> Vec3 {
    Vec3::new(cx + 0.5, height + 1.0, cz + 0.5)
}

/// Whether a hit on a crystal is taken: fire is refused, and a shielded crystal takes only a creative player's hit.
pub fn hit_takes(fire: bool, shielded: bool, creative: bool) -> bool {
    !fire && (!shielded || creative)
}
