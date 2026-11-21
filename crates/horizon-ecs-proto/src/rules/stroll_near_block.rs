//! Calculs de la flânerie près d'un bloc mémorisé (`StrollAroundPoi`) : la distance horizontale au centre du bloc.
use horizon_core::geometry::Vec3;

/// The point `at` is within `d` blocks, sideways, of the centre of the block `p`.
pub fn near_centre(p: [i32; 3], at: Vec3, d: f64) -> bool {
    let (dx, dz) = (f64::from(p[0]) + 0.5 - at.x, f64::from(p[2]) + 0.5 - at.z);
    dx * dx + dz * dz <= d * d
}
