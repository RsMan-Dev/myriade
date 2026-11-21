//! The lookout: the wait of the next ask, the count of looks, their direction and their length.

/// The ask's period in asks: one ask in `1 / chance` passes, at least one ask.
pub fn ask_period(chance: f64) -> i32 {
    (1.0 / chance).round().max(1.0) as i32
}
/// The number of looks of a lookout, from a draw in `0..3`: two to four.
pub fn look_count(draw: i32) -> i32 {
    2 + draw
}
/// The direction of a look from its angle: the unit vector on the ground.
pub fn look_dir(angle: f64) -> [f64; 2] {
    [angle.cos(), angle.sin()]
}
/// The ticks of a look from its draw in `0..20`: 80 to 99, halved and rounded up.
pub fn look_ticks(draw: i32) -> i32 {
    (80 + draw + 1) / 2
}
