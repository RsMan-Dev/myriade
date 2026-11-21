//! The flight of a thrown eye of ender (`EyeOfEnder.updateDeltaMovement`).
use horizon_core::geometry::Vec3;

/// `EyeOfEnder.updateDeltaMovement`: the horizontal speed closes 0.25 % of its gap to the target per tick, slower under a block
/// away; the vertical speed drifts 1.5 % towards up or down, by whether the eye is below the target's height.
pub fn update_delta(old: Vec3, pos: Vec3, target: Vec3) -> Vec3 {
    let (hx, hz) = (target.x - pos.x, target.z - pos.z);
    let length = hx.hypot(hz);
    let old_h = old.x.hypot(old.z);
    let mut speed = old_h + 0.0025 * (length - old_h);
    let mut y = old.y;
    if length < 1.0 {
        speed *= 0.8;
        y *= 0.8;
    }
    let wanted = if pos.y - old.y < target.y { 1.0 } else { -1.0 };
    Vec3::new(hx * (speed / length), y + (wanted - y) * 0.015, hz * (speed / length))
}
