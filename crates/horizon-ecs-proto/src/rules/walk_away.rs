//! The walk away's tests: the squared distance to the point to keep away from, and whether its memory still holds.

/// The squared distance between two points.
pub fn dist_sqr(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// Whether the remembered point is still remembered at `now` (its ttl is `until`).
pub fn still_avoids(now: u64, until: u64) -> bool {
    now < until
}

/// Whether `from` is strictly nearer to `me` than `desired` (squared comparison, no root).
pub fn is_nearer_than(from: [f64; 3], me: [f64; 3], desired: f64) -> bool {
    let (dx, dy, dz) = (from[0] - me[0], from[1] - me[1], from[2] - me[2]);
    dx * dx + dy * dy + dz * dz < desired * desired
}
