//! The door of a creature: the centre test of a block, the horizontal reach of a door, and the other half of a door.
use horizon_core::geometry::Vec3;

/// `BlockPos.closerToCenterThan`: the block's centre is nearer to `at` than `distance`.
pub fn closer_to_center(p: [i32; 3], at: Vec3, distance: f64) -> bool {
    let (dx, dy, dz) = (f64::from(p[0]) + 0.5 - at.x, f64::from(p[1]) + 0.5 - at.y, f64::from(p[2]) + 0.5 - at.z);
    dx * dx + dy * dy + dz * dz < distance * distance
}

/// A door is within 1.5 blocks horizontally: the squared horizontal offset is at most 2.25.
pub fn near_door_column(dx: f64, dz: f64) -> bool {
    dx * dx + dz * dz <= 2.25
}

/// The other half of a door at `p`: one block up when this half is the lower one, else one block down.
pub fn other_half(p: [i32; 3], lower: bool) -> [i32; 3] {
    [p[0], p[1] + if lower { 1 } else { -1 }, p[2]]
}
