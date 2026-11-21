//! Aim of a sulfur cube searching for an item (`SulfurCube.SearchForItemsGoal`): the bearing of the point, turned toward.
use horizon_core::geometry::{Vec3, trig};

/// The hop's direction that points from `pos` to `point` (degrees, as the hop holds it).
pub fn aim_degrees(point: Vec3, pos: Vec3) -> f32 {
    trig::atan2_degrees(point.z - pos.z, point.x - pos.x) - 90.0
}

/// `Mth.rotlerp`: turns `from` toward `to` by at most `max` degrees, along the short way.
pub fn rotlerp(from: f32, to: f32, max: f32) -> f32 {
    let mut d = (to - from).rem_euclid(360.0);
    if d > 180.0 {
        d -= 360.0;
    }
    from + d.clamp(-max, max)
}
