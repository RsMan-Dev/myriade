//! Calculations of the walk back into a restriction (`Trader.MoveTowardsRestrictionGoal`): the home test and the point the
//! animal walks toward (`goals::within_home` of the `Mob`, `trader.rs:277-325`).

/// `Mob.isWithinHome`: the floored position is within the radius of the home block: `d^2 < radius^2` on the floored position.
pub fn within_home(center: [i32; 3], radius: i32, p: [f64; 3]) -> bool {
    let pos = [p[0].floor() as i32, p[1].floor() as i32, p[2].floor() as i32];
    let d = |i: usize| f64::from(center[i]) - f64::from(pos[i]);
    d(0) * d(0) + d(1) * d(1) + d(2) * d(2) < f64::from(radius.wrapping_mul(radius))
}
/// The point the draw of the walk is made toward: the centre of the home block (x and z at the middle of the block).
pub fn home_target(center: [i32; 3]) -> [f64; 3] {
    [f64::from(center[0]) + 0.5, f64::from(center[1]), f64::from(center[2]) + 0.5]
}
