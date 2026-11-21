//! Calculations of the pounce on a target (`OcelotAttackGoal`, `tame.rs:1023` and `1065`): the reach of the strike, the speed
//! of the walk by distance band, the strike's cool-down, and the damage of a strike.

/// The reach of the strike, as a squared distance: twice the body width, squared.
pub fn melee_reach2(width: f32) -> f64 {
    let reach = f64::from(width) * 2.0;
    reach * reach
}
/// The walk speed for a squared distance `d2`: `near` between the strike reach and `band`, `slow` between `band` and `within`,
/// `far` beyond `within`. All distances are squared.
pub fn pounce_speed(d2: f64, melee: f64, band: f64, within: f64, near: f64, slow: f64, far: f64) -> f64 {
    if d2 > melee && d2 < band {
        near
    } else if d2 < within {
        slow
    } else {
        far
    }
}
/// The strike's cool-down after one tick: the ticks left, not under zero.
pub fn cool_down(left: i32) -> i32 {
    (left - 1).max(0)
}
/// The damage of a strike: the entity's attack damage, one when it is nil or less.
pub fn strike_amount(attack: f32) -> f32 {
    if attack <= 0.0 { 1.0 } else { attack }
}
