//! Calculs de la lave : le marcheur flotte-t-il à la surface, et sa vitesse quand il est plus bas.
use horizon_core::geometry::Vec3;

/// `Strider.floatStrider`: the animal at height `py`, in the lava block row `y`, is at the surface (its feet above the row's middle).
pub fn at_surface(py: f64, y: i32) -> bool {
    py > f64::from(y) + 0.5 - f64::from(1.0e-5_f32)
}
/// `Strider.floatStrider`: deeper in the lava the velocity is halved and lifted.
pub fn lifted(u: Vec3) -> Vec3 {
    Vec3::new(u.x * 0.5, u.y * 0.5 + 0.05, u.z * 0.5)
}
