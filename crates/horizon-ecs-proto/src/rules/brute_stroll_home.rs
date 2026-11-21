//! Calculations of the brute's walk home: the band of distances where it applies, and the wait before the next try.
use horizon_core::geometry::Vec3;

/// Whether the animal at `p` lies in the band where the walk home applies: its squared distance to the centre of the home block
/// is more than `near` and less than `far` blocks.
pub fn in_home_band(p: Vec3, block: [i32; 3], near: f64, far: f64) -> bool {
    let c = Vec3::new(f64::from(block[0]) + 0.5, f64::from(block[1]), f64::from(block[2]) + 0.5);
    let d2 = (p.x - c.x).powi(2) + (p.y - c.y).powi(2) + (p.z - c.z).powi(2);
    d2 > near * near && d2 < far * far
}
/// The ticks before the next try, from the time `next_ok` the gate opens at: at least one tick.
pub fn retry_wait(next_ok: u64, now: u64) -> i32 {
    next_ok.saturating_sub(now).max(1) as i32
}
