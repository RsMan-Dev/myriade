//! Calculations of the search areas: the circle that covers a box.
use horizon_core::geometry::Aabb;

/// The centre (x, z) of the box and the radius of the circle that covers its footprint (half its widest side).
pub fn centre_radius(a: Aabb) -> ((f64, f64), f64) {
    let (c, d) = (a.center(), a.size());
    ((c.x, c.z), d.x.max(d.z) / 2.0)
}
