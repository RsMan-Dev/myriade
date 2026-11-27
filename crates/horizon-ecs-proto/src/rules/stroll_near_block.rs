//! Calculs de la flânerie près d'un bloc mémorisé (`StrollAroundPoi`) : la distance au centre du bloc.
use horizon_core::geometry::Vec3;

/// The point `at` is strictly within `d` blocks of the centre of the block `p`, height included (`BlockPos.closerToCenterThan`).
pub fn near_centre(p: [i32; 3], at: Vec3, d: f64) -> bool {
    let (dx, dy, dz) = (f64::from(p[0]) + 0.5 - at.x, f64::from(p[1]) + 0.5 - at.y, f64::from(p[2]) + 0.5 - at.z);
    dx * dx + dy * dy + dz * dz < d * d
}
