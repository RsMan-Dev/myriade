//! The dolphin's trip to its treasure: the treasure's centre, and the distance tests that end the trip or ask the next step.
use horizon_core::geometry::Vec3;

/// The centre of the treasure block: `+ 0.5` on each axis.
pub fn treasure_centre(block: [i32; 3]) -> [f64; 3] {
    [f64::from(block[0]) + 0.5, f64::from(block[1]) + 0.5, f64::from(block[2]) + 0.5]
}

/// Whether the dolphin at `p` is within `arrive_sqr` (squared) of the treasure, on its row `floor(y) + 0.5`.
pub fn reached_treasure(p: Vec3, treasure: [i32; 3], arrive_sqr: f64) -> bool {
    let to = treasure_centre(treasure);
    let here = Vec3::new(p.x, p.y.floor() + 0.5, p.z);
    (to[0] - here.x).powi(2) + (to[1] - here.y).powi(2) + (to[2] - here.z).powi(2) < arrive_sqr
}

/// Whether the dolphin at `p` is within `close_sqr` (squared) of the path's target block centre (x and z `+ 0.5`).
pub fn near_path_target(p: Vec3, target: [f64; 3], close_sqr: f64) -> bool {
    let t = Vec3::new(target[0] + 0.5, target[1], target[2] + 0.5);
    (p.x - t.x).powi(2) + (p.y - t.y).powi(2) + (p.z - t.z).powi(2) < close_sqr
}
