//! The reach and the scan box of a live area cloud: what `tick_live` searches for the players and creatures it may touch.
use horizon_core::geometry::{Aabb, Vec3};

/// The reach of a cloud of `radius` growing by `radius_per_tick`: the larger of its radius now and after this tick's growth.
pub fn cloud_reach(radius: f32, radius_per_tick: f32) -> f32 {
    radius.max(radius + radius_per_tick)
}

/// The box a cloud at `at` scans: `reach` around its floor, half a block high.
pub fn scan_box(at: [f64; 3], reach: f32) -> Aabb {
    let [x, y, z] = at;
    Aabb::new(Vec3::new(x - f64::from(reach), y, z - f64::from(reach)), Vec3::new(x + f64::from(reach), y + 0.5, z + f64::from(reach)))
}
