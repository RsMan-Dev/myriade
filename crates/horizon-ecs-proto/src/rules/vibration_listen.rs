//! The vibration's reach: the listening range, the squared distance from the source, and the delay before the ear hears it.

/// The listening range in blocks (`VibrationUser` of the warden: 16).
pub const RANGE: f64 = 16.0;

/// The block that holds a point: each coordinate floored.
pub fn block_of(p: [f64; 3]) -> [i32; 3] {
    p.map(|c| c.floor() as i32)
}

/// The squared distance between two points.
pub fn dist_sqr(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// Whether a squared distance is within the listening range.
pub fn in_range(d2: f64) -> bool {
    d2 <= RANGE * RANGE
}

/// The ticks of the delay before the listener hears the vibration: its distance in blocks, truncated.
pub fn hear_delay(d2: f64) -> i32 {
    d2.sqrt() as i32
}
