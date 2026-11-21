//! The teleport of an attached body: the cells and boxes of its spots, its foot on a block, and the tests of hurt and of copy.
use horizon_core::geometry::{Aabb, Vec3};

/// The block cell of a position (floored on each axis).
pub fn cell_of(p: Vec3) -> [i32; 3] {
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}

/// The minimum corner of block `t`.
pub fn corner_of(t: [i32; 3]) -> Vec3 {
    Vec3::new(f64::from(t[0]), f64::from(t[1]), f64::from(t[2]))
}

/// The unit box of block `t`.
pub fn spot_box(t: [i32; 3]) -> Aabb {
    Aabb::new(corner_of(t), corner_of(t).add_vector(Vec3::new(1.0, 1.0, 1.0)))
}

/// Where a body stands on block `t`: centred on x and z, at the block's floor on y.
pub fn foot_of(t: [i32; 3]) -> Vec3 {
    Vec3::new(f64::from(t[0]) + 0.5, f64::from(t[1]), f64::from(t[2]) + 0.5)
}

/// The margin the boxes are shrunk by before the tests.
pub fn eps() -> Vec3 {
    Vec3::new(1.0e-6, 1.0e-6, 1.0e-6)
}

/// The box of a body of `size`, fully open from the face whose step is `step` on block `p`.
pub fn attachment_box(p: [i32; 3], step: [i32; 3], size: f32) -> Aabb {
    let s = step.map(f64::from);
    let size = f64::from(size);
    let half = size * 0.5;
    Aabb::new(Vec3::new(-half, 0.0, -half), Vec3::new(half, size, half))
        .expand_towards(Vec3::new(s[0] * size, s[1] * size, s[2] * size))
        .move_by(foot_of(p))
        .deflate(eps())
}

/// Below `below` of the max health: the body may teleport (`hp < max * below`).
pub fn is_hurt_below(hp: f32, max_hp: f32, below: f32) -> bool {
    hp < max_hp * below
}

/// The copy is refused when the draw is under `others / divisor` (one chance less per other body).
pub fn copy_refused(draw: f32, others: f32, divisor: f32) -> bool {
    draw < others / divisor
}
