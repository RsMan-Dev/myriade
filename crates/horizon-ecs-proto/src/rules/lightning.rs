//! The numbers of a strike: the box a body must meet to be struck, the spots a strike sets on fire, the block under the strike point.
use horizon_core::geometry::Vec3;
use horizon_core::random::{LegacyRandom, RandomSource};

/// Half the widest body a bolt can strike (the index is by centre).
pub const MAX_HALF_WIDTH: f64 = 4.0;

/// Whether a body centred at `c` (its feet at `c.y`) meets the strike box: 3 blocks around, 3 down, 9 up, of the point `at`.
pub fn strike_hit(c: Vec3, width: f32, height: f32, at: [f64; 3]) -> bool {
    let (hw, hh) = (f64::from(width) / 2.0, f64::from(height));
    c.x - hw < at[0] + 3.0 && c.x + hw > at[0] - 3.0 && c.y < at[1] + 9.0 && c.y + hh > at[1] - 3.0 && c.z - hw < at[2] + 3.0 && c.z + hw > at[2] - 3.0
}
/// The spots a strike sets on fire: the base block, then `extra` neighbours drawn from the bolt's generator (three draws each).
pub fn fire_spots(base: [i32; 3], extra: i32, rng: &mut LegacyRandom) -> Vec<[i32; 3]> {
    let mut spots = vec![base];
    for _ in 0..extra {
        let mut d = || rng.next_int_bound(3).unwrap_or(1) - 1;
        spots.push([base[0] + d(), base[1] + d(), base[2] + d()]);
    }
    spots
}
/// The block under a strike point (a point on a block's top face counts as the block below).
pub fn under(at: [f64; 3]) -> [i32; 3] {
    [at[0].floor() as i32, (at[1] - 1.0e-6).floor() as i32, at[2].floor() as i32]
}
