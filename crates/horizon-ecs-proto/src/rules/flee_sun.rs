//! The hiding spot of a burning monster: the offset of a try around it, its brightness test, and the centre of its block.

/// The offset of one try of `hide_pos` from its three draws: x in -10..10, y in -3..3, z in -10..10.
pub fn hide_offset(x: i32, y: i32, z: i32) -> (i32, i32, i32) {
    (x - 10, y - 3, z - 10)
}

/// A spot is bright enough when its brightness is above 0.5.
pub fn bright_enough(brightness: f64) -> bool {
    brightness > 0.5
}

/// The centre of a block along one axis, the point the walk aims at.
pub fn centre(v: i32) -> f64 {
    f64::from(v) + 0.5
}
