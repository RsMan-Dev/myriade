//! Geometry of `go_to_wanted`: the search box around the creature, the wake box of a dropped item, and the reach at arrival.
use horizon_core::geometry::{Aabb, Vec3};

/// The most a pocket takes of one stack: the stack size of the items the allay takes (64 here, the registry is not read).
pub const MAX_PICK: u32 = 64;
/// The widest `range` of the goals that use `hand`: a dropped item farther than this does not wake anyone.
pub const WAKE_RANGE: f64 = 32.0;

/// The box the wanted items are searched in: `range` blocks around `at`.
pub fn search_area(at: Vec3, range: f64) -> Aabb {
    Aabb::new(Vec3::new(at.x - range, at.y - range, at.z - range), Vec3::new(at.x + range, at.y + range, at.z + range))
}

/// A dropped item at `at` wakes a creature at `me` whose `range` box holds it.
pub fn in_wake_box(me: [f64; 3], at: [f64; 3], range: f64) -> bool {
    (me[0] - at[0]).abs() <= range && (me[1] - at[1]).abs() <= range && (me[2] - at[2]).abs() <= range
}

/// The item is too far to be taken at the end of the path (reach 1.5 blocks, squared distance over 2.25).
pub fn out_of_reach(dist_sqr: f64) -> bool {
    dist_sqr > 2.25
}
