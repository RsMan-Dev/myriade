//! Calculations of the wither's block breaking (`WitherBoss.customServerAiStep`, `counter2`): the countdown's deadline and
//! the box of cells it breaks.

/// The tick the countdown ends, `delay` ticks after `now` (no negative delay).
pub fn countdown_due(now: u64, delay: i32) -> u64 {
    now + delay.max(0) as u64
}

/// The half-width (x and z, each side) and the height (y, from the feet up) of the box of cells broken, from the body's
/// width and height in blocks.
pub fn break_box(width: f64, height: f64) -> (i32, i32) {
    ((width / 2.0 + 1.0).floor() as i32, height.floor() as i32)
}
