//! Calculations of the random look (`RandomLook`): the end of a scan, and its direction from an angle.

/// The tick a scan ends: twenty ticks plus the draw `n` (under 20), and one.
pub fn scan_until(now: u64, n: i32) -> u64 {
    now + 20 + n as u64 + 1
}
/// The horizontal direction `(dx, dz)` of a scan at the angle `a` (radians).
pub fn scan_dir(a: f64) -> (f64, f64) {
    (a.cos(), a.sin())
}
