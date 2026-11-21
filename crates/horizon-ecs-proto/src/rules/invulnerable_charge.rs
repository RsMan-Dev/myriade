//! Timing and blast point of the wither's charge (`WitherBoss.makeInvulnerable`).
use horizon_core::geometry::Vec3;

/// The tick the charge ends: `ticks` (not negative) after `now`.
pub fn charge_until(now: u64, ticks: i32) -> u64 {
    now + ticks.max(0) as u64
}

/// The blast comes from the eyes: `eye` above the feet at `pos`.
pub fn blast_at(pos: Vec3, eye: f64) -> Vec3 {
    Vec3::new(pos.x, pos.y + eye, pos.z)
}
