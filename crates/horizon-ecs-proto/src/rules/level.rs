//! The numbers of a block-breaking and trampling read: the jump's speed and the face a box covers.
use horizon_core::geometry::Aabb;

/// `LivingEntity.getJumpPower` with the default jump strength and no boost.
pub const JUMP_POWER: f64 = 0.42;

/// The vertical speed a jump gives: the jump power, unless the entity already rises faster.
pub fn jump_speed(vy: f64) -> f64 {
    JUMP_POWER.max(vy)
}
/// Whether a block's collision box covers the face `face` (down, up, north, south, west, east: even faces are the low side). The box
/// stands for the shape: an approximation of `isFaceSturdy`.
pub fn full_face_of(b: Aabb, face: u8) -> bool {
    let (lo, hi) = (b.min_position(), b.max_position());
    let (lo, hi) = ([lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z]);
    let axis = match face {
        0 | 1 => 1,
        2 | 3 => 2,
        _ => 0,
    };
    let touches = if face & 1 == 1 { hi[axis] >= 1.0 } else { lo[axis] <= 0.0 };
    touches && (0..3).filter(|&a| a != axis).all(|a| lo[a] <= 0.0 && hi[a] >= 1.0)
}
