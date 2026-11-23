//! Where a sleeper lies on a bed (`LivingEntity.setPosToBed`: the bed's height 0.5625 plus 0.125; the jar's bytecode, no `pos_to_bed`).
use horizon_core::geometry::Vec3;

/// The place of a body lying on the bed at `p`: the middle of the block, at the bed's height and 0.125 over it.
pub fn lie_spot(p: [i32; 3]) -> Vec3 {
    Vec3::new(f64::from(p[0]) + 0.5, f64::from(p[1]) + 0.5625 + 0.125, f64::from(p[2]) + 0.5)
}
