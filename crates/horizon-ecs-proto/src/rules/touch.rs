use horizon_core::geometry::{Aabb, Vec3};

/// The box a toucher reaches: its box grown by `reach` on each axis.
pub fn reach_box(b: Aabb, reach: [f64; 3]) -> Aabb {
    b.inflate(Vec3::new(reach[0], reach[1], reach[2]))
}
