//! `PoiAssign` (`brain/behaviours/poi_assign.rs`): the reach of the entity to the point of interest it claims.
use horizon_core::geometry::Vec3;

/// Whether the centre of the block `s` is within 2 blocks of `at` (4 squared, strictly less).
pub fn near(s: [i32; 3], at: Vec3) -> bool {
    let centre = Vec3::new(f64::from(s[0]) + 0.5, f64::from(s[1]) + 0.5, f64::from(s[2]) + 0.5);
    at.distance_squared(centre) < 4.0
}
