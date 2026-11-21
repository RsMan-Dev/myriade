use horizon_core::geometry::Vec3;

/// `Entity.hasMovedHorizontallyRecently`: the horizontal part of the speed is over 1e-5.
pub fn horizontally_moved(velocity: Vec3) -> bool {
    (velocity.x * velocity.x + velocity.z * velocity.z).sqrt() > 1.0e-5
}
