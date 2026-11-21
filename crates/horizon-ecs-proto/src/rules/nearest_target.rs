//! Calculations of the nearest-target search (`NearestAttackableTargetGoal`): the delay of the selector's ask, the reach of
//! a player goal, the sphere and box tests of the candidates, and the watch that wakes a parked player goal.

/// `LivingEntity.getVisibilityPercent` of an invisible target with no armour: `0.7 * 0.1`.
pub const INVISIBLE: f64 = 0.07;
/// The furthest a player watch sees: a longer reach is polled instead of watched.
pub const WATCH_MAX: f64 = 64.0;

/// `reducedTickDelay`: half of `n`, rounded up.
pub fn reduced(n: i32) -> i32 {
    (n + 1) / 2
}
/// The first ask of an armed instance: twice a geometric draw over the reduced interval (`draw(bound)` is the entity's stream).
pub fn arm_wait(draw: impl FnOnce(i32) -> i32, interval: i32) -> i32 {
    2 * draw(reduced(interval))
}
/// The heal goal's draw: one chance in two (`draw(bound)` is the entity's stream).
pub fn heal_chance(draw: impl FnOnce(i32) -> i32) -> bool {
    draw(2) == 0
}
/// The reach of an instance: its own range when it has one, else the follow range times the scale (16 without a follow range).
pub fn reach_range(range: Option<f64>, follow_range: Option<f64>, follow_scale: f64) -> f64 {
    range.unwrap_or_else(|| follow_range.unwrap_or(16.0) * follow_scale)
}
/// True when the offset `(dx, dy, dz)` is within the sphere of radius `range` (squared distance at most `range^2`).
pub fn in_sphere(dx: f64, dy: f64, dz: f64, range: f64) -> bool {
    dx.powi(2) + dy.powi(2) + dz.powi(2) <= range * range
}
/// True when the offset `(dx, dy, dz)` is within the box of half-size `range` on each axis.
pub fn in_box(dx: f64, dy: f64, dz: f64, range: f64) -> bool {
    dx <= range && dy <= range && dz <= range
}
/// The squared length of the offset `(dx, dy, dz)`.
pub fn dist2(dx: f64, dy: f64, dz: f64) -> f64 {
    dx * dx + dy * dy + dz * dz
}
/// True when a reach is too long for a player watch (`range + 1` beyond `WATCH_MAX`): the goal is polled.
pub fn watch_polled(range: f64) -> bool {
    range + 1.0 > WATCH_MAX
}
/// The radius of the player watch laid for a reach: one block more than the reach.
pub fn watch_radius(range: f64) -> f32 {
    (range + 1.0) as f32
}
