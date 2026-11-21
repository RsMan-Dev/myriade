//! The fox's nap: the wait of a countdown, and the reach of the creatures and players that wake it.

/// The wait of a countdown of `asks` asks: two ticks each, plus the ask that looks at the conditions.
pub fn countdown_wait(asks: i32) -> i32 {
    2 * (asks + 1)
}
/// `Fox.alertable`: a creature or a player within 6 blocks of height (`dy` the vertical offset).
pub fn alarm_height(dy: f64) -> bool {
    dy.abs() <= 6.0
}
/// `Fox.alertable`: a creature within 12 blocks (`dist_sq` the squared distance).
pub fn creature_alarm_reach(dist_sq: f64) -> bool {
    dist_sq <= 144.0
}
/// `Fox.alertable`: a player within `range` blocks (`dist_sq` the squared distance).
pub fn player_alarm_reach(dist_sq: f64, range: f64) -> bool {
    dist_sq <= range * range
}
/// `Fox.alertable`: the range of a player's alarm, 12 blocks times its visibility, at least 2.
pub fn alarm_range(visibility: f64) -> f64 {
    (12.0 * visibility).max(2.0)
}
