//! The wander's hops: the point a hop leads to, and the block centre a short walk leads to.
use horizon_core::geometry::Vec3;

/// The point a hop of `hop` blocks leads to on the way to the floored `p`, from the creature at `own`.
pub fn hop_toward(p: [f64; 3], own: Vec3, hop: f64) -> [f64; 3] {
    let d = Vec3::new(p[0].floor() - own.x, p[1].floor() - own.y, p[2].floor() - own.z);
    let len = (d.x * d.x + d.y * d.y + d.z * d.z).sqrt().max(1e-9);
    [d.x / len * hop + own.x, d.y / len * hop + own.y, d.z / len * hop + own.z]
}

/// Each coordinate of `p` floored.
pub fn floored(p: [f64; 3]) -> [f64; 3] {
    [p[0].floor(), p[1].floor(), p[2].floor()]
}
