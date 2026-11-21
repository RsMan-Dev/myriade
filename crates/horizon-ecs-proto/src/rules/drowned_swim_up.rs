//! The drowned's rise: the depth tests against the sea, the closeness to the path's target, and the point above it.
use horizon_core::geometry::Vec3;

/// The drowned is below the surface: under `sea - 1`.
pub fn below_surface(y: f64, sea: f64) -> bool {
    y < sea - 1.0
}

/// The drowned is deep enough to rise: under `sea - 2`.
pub fn deep_enough(y: f64, sea: i32) -> bool {
    y < f64::from(sea - 2)
}

/// Close to the path's target: the squared distance is under 4.
pub fn close_to_target(p: Vec3, t: [f64; 3]) -> bool {
    (p.x - t[0]).powi(2) + (p.y - t[1]).powi(2) + (p.z - t[2]).powi(2) < 4.0
}

/// The point the rise walks toward: over the drowned at the surface (`sea - 1`).
pub fn rise_target(p: Vec3, sea: f64) -> [f64; 3] {
    [p.x, sea - 1.0, p.z]
}
