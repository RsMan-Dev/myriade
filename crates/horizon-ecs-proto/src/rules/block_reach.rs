//! Block reach of a villager's block behaviours: the block a body stands in, the 3x3x3 around it, the centre of a block, and
//! the sky test above it (`villager_brain.rs:2497-2529`, `no_blocks_above`).
use myriade_engine_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;

/// The block the position is in.
pub fn block_of(x: f64, y: f64, z: f64) -> [i32; 3] {
    [x.floor() as i32, y.floor() as i32, z.floor() as i32]
}

/// The centre of block `p`.
pub fn centre(p: [i32; 3]) -> Vec3 {
    Vec3::new(f64::from(p[0]) + 0.5, f64::from(p[1]) + 0.5, f64::from(p[2]) + 0.5)
}

/// The 27 blocks of the 3x3x3 around `c`, `c` included, in the order x, y, z.
pub fn neighbourhood(c: [i32; 3]) -> Vec<[i32; 3]> {
    (-1..=1).flat_map(|x| (-1..=1).flat_map(move |y| (-1..=1).map(move |z| [c[0] + x, c[1] + y, c[2] + z]))).collect()
}

/// `no_blocks_above`: the block sees the sky and the motion-blocking height of its column is not above `y`.
pub fn no_blocks_above(mw: &MobWorld<'static>, p: [i32; 3], y: f64) -> bool {
    mw.can_see_sky(p[0], p[1], p[2]) && f64::from(mw.motion_blocking_height(p[0], p[2])) <= y
}
