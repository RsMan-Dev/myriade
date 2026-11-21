//! The escort of a vehicle: the horizontal step of a yaw, and the points behind and ahead of the rider's block.

/// `Entity.getMotionDirection` / `Direction.fromYRot`: the horizontal step of the yaw (south, west, north, east).
pub fn motion_step(yaw: f32) -> [i32; 2] {
    match (yaw / 90.0 + 0.5).floor() as i32 & 3 {
        0 => [0, 1],
        1 => [-1, 0],
        2 => [0, -1],
        _ => [1, 0],
    }
}

/// The point one block behind the rider's block, one lower.
pub fn behind(block: [i32; 3], dir: [i32; 2]) -> [f64; 3] {
    [f64::from(block[0] - dir[0]), f64::from(block[1] - 1), f64::from(block[2] - dir[1])]
}

/// The point `lead` blocks ahead of the rider's block, one lower.
pub fn ahead(block: [i32; 3], dir: [i32; 2], lead: i32) -> [f64; 3] {
    [f64::from(block[0] + dir[0] * lead), f64::from(block[1] - 1), f64::from(block[2] + dir[1] * lead)]
}
