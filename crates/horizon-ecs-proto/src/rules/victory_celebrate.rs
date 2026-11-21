//! The raid victory's celebration: its end time, whether it is over, and where a firework goes off.
use horizon_core::geometry::Vec3;

/// The tick the celebration ends: `duration` ticks after `now`.
pub fn celebration_end(now: u64, duration: i64) -> u64 {
    now + duration as u64
}

/// Whether the celebration is over at `now`.
pub fn is_over(now: u64, until: u64) -> bool {
    now >= until
}

/// The point a firework goes off at: two blocks above the villager.
pub fn firework_at(p: Vec3) -> [f64; 3] {
    [p.x, p.y + 2.0, p.z]
}
