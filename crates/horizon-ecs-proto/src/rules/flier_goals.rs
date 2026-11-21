//! The fliers' goals: when the vex charges and keeps steering, the phantom's scan box, its dive and circle points, and the heights kept.
use horizon_core::geometry::trig::Mth;

/// `VexChargeAttackGoal` may start: nothing wanted and the target farther than 2 blocks (the offsets `dx, dy, dz` to the target).
pub fn charge_ready(wanted: bool, dx: f64, dy: f64, dz: f64) -> bool {
    !wanted && dx.powi(2) + dy.powi(2) + dz.powi(2) > 4.0
}
/// `VexChargeAttackGoal.tick`: the charge keeps steering at the target while it is within 3 blocks.
pub fn charge_steers(dx: f64, dy: f64, dz: f64) -> bool {
    dx.powi(2) + dy.powi(2) + dz.powi(2) < 9.0
}
/// `Mob.doHurtTarget`: the strike is the attack damage, at least 1.
pub fn strike_amount(attack: f32) -> f32 {
    attack.max(1.0)
}
/// `PhantomAttackPlayerTargetGoal`: the reach of the scan, 16 blocks and the width of the phantom.
pub fn scan_range(width: f64) -> f64 {
    16.0 + width
}
/// The scan box: within `r` horizontally and 64 blocks vertically of the phantom (the offsets `dx, dy, dz` to the player).
pub fn in_scan_box(dx: f64, dy: f64, dz: f64, r: f64) -> bool {
    dx.abs() <= r && dy.abs() <= 64.0 && dz.abs() <= r
}
/// `PhantomAttackStrategyGoal`: the anchor 20 to 39 blocks above the target's floor, never under the sea level plus one (`offset` drawn by the caller).
pub fn anchor_y(floor_y: i32, offset: i32, sea: i32) -> i32 {
    (floor_y + 20 + offset).max(sea + 1)
}
/// `PhantomAttackStrategyGoal.stop`: the anchor 10 to 29 blocks above the motion-blocking top (`offset` drawn by the caller).
pub fn drop_anchor_y(top: i32, offset: i32) -> i32 {
    top + 10 + offset
}
/// `PhantomSweepAttackGoal`: the wanted height of the dive, the middle of the target's box.
pub fn sweep_mid(min_y: f64, max_y: f64) -> f64 {
    (min_y + max_y) * 0.5
}
/// `PhantomCircleAroundAnchorGoal.selectNext`: the wanted point, `distance` blocks round the anchor at `angle`, `height` under 4 blocks.
pub fn circle_point(anchor: [i32; 3], distance: f32, height: f32, angle: f32) -> [f64; 3] {
    [f64::from(anchor[0]) + f64::from(distance * angle.mcos()), f64::from(anchor[1]) + f64::from(-4.0 + height), f64::from(anchor[2]) + f64::from(distance * angle.msin())]
}
/// `PhantomCircleAroundAnchorGoal`: the wanted point is reached within 2 blocks (the offsets `dx, dy, dz` to the point).
pub fn circle_reached(dx: f64, dy: f64, dz: f64) -> bool {
    dx.powi(2) + dy.powi(2) + dz.powi(2) < 4.0
}
/// `PhantomCircleAroundAnchorGoal`: above a solid floor, the height is at least 1.
pub fn raised_height(height: f32) -> f32 {
    height.max(1.0)
}
/// `PhantomCircleAroundAnchorGoal`: under a solid ceiling, the height is at most -1.
pub fn lowered_height(height: f32) -> f32 {
    height.min(-1.0)
}
/// `PhantomCircleAroundAnchorGoal` may start: without a target or out of the dive.
pub fn circle_may_start(has_target: bool, diving: bool) -> bool {
    !has_target || !diving
}
/// `PhantomSweepAttackGoal` may start: diving with a target.
pub fn sweep_may_start(diving: bool, has_target: bool) -> bool {
    diving && has_target
}
