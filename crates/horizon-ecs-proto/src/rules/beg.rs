//! Calculations of the wolf's beg goal (`mobs/tame.rs:610-668`): the wake-up radius, the distance of a player, the range test,
//! and the ticks of a beg.
use horizon_core::geometry::Vec3;

/// The radius, in blocks, within which a change of a player wakes the wolves (`Params::DEFAULT` range is 8 blocks).
pub const REACH: f64 = 8.0;

/// The squared distance from `a` to the player at `(x, y, z)`.
pub fn dist_sqr(a: Vec3, x: f64, y: f64, z: f64) -> f64 {
    (x - a.x).powi(2) + (y - a.y).powi(2) + (z - a.z).powi(2)
}

/// The player is in range: the squared distance `d` is within `range_sqr`.
pub fn in_range(d: f64, range_sqr: f64) -> bool {
    d <= range_sqr
}

/// `reducedTickDelay(asks + n)`: the ticks of a beg, `n` being the draw of `nextInt(asks)`.
pub fn beg_ticks(asks: i32, n: i32) -> i32 {
    (asks + n + 1) / 2
}
