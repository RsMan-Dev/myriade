//! `InteractWithDoor` (`brain/behaviours/interact_with_door.rs`): the step counter, the reach that closes a passed door, and the half offset.
use horizon_core::geometry::Vec3;

/// The step counter of `try_start`: the same node asked again sets `cooldown` to `reset`; another node counts it down.
/// Returns the new counter and whether the step is taken (the counter is 0 or below).
pub fn step_gate(last: Option<[i32; 3]>, to: [i32; 3], cooldown: i64, reset: i64) -> (i64, bool) {
    if last == Some(to) {
        (reset, true)
    } else {
        let c = cooldown - 1;
        (c, c <= 0)
    }
}

/// Whether the door at cell `door` is within reach of `me`: its centre is closer than `range_sqr` (squared distance).
pub fn in_close_range(me: Vec3, door: [i32; 3], range_sqr: f64) -> bool {
    let centre = Vec3::new(f64::from(door[0]) + 0.5, f64::from(door[1]) + 0.5, f64::from(door[2]) + 0.5);
    me.distance_squared(centre) < range_sqr
}

/// The `y` offset of the other half of a door: -1 for the upper half, else +1.
pub fn other_half_dy(upper: bool) -> i32 {
    if upper { -1 } else { 1 }
}
