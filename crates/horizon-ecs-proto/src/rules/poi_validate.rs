//! `PoiValidate` (`brain/behaviours/poi_validate.rs`): the range in which a point of interest is still near the entity.
use horizon_core::geometry::Vec3;

/// The range in blocks.
pub const RANGE: f64 = 16.0;

/// Whether the centre of the block `p` is within `RANGE` of `at`.
pub fn near(p: [i32; 3], at: Vec3) -> bool {
    let centre = Vec3::new(f64::from(p[0]) + 0.5, f64::from(p[1]) + 0.5, f64::from(p[2]) + 0.5);
    at.distance_squared(centre) < RANGE * RANGE
}
