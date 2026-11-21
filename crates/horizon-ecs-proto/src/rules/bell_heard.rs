//! Hearing a bell (`HEARD_BELL`): the range it is heard at, and the centre of the bell's column.

/// The range a bell is heard at, in blocks.
pub const RANGE: f64 = 32.0;

/// The horizontal centre of the block at `pos` (x, z), where the ring is heard from.
pub fn centre(pos: [i32; 3]) -> (f64, f64) {
    (f64::from(pos[0]) + 0.5, f64::from(pos[2]) + 0.5)
}
