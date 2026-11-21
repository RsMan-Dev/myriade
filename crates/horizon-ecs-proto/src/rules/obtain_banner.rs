//! Calculations of the raider's banner fetch (`Raider.ObtainRaidLeaderBannerGoal`): the box of the banner search, the reach that
//! wakes the raiders near a dropped item, and the distance at which the banner is taken.
use horizon_core::geometry::{Aabb, Vec3};

/// Horizontal reach of the banner search, and of the wake of the raiders around a dropped item (blocks).
pub const REACH_H: f64 = 16.0;
/// Vertical reach of the banner search (blocks).
pub const REACH_V: f64 = 8.0;
/// The squared distance under which the banner is taken on arrival (1.5 blocks).
pub const PICKUP_REACH2: f64 = 2.25;

/// The box of the banner search around the raider at `p`: `REACH_H` on x and z, `REACH_V` on y.
pub fn banner_area(p: Vec3) -> Aabb {
    Aabb::new(Vec3::new(p.x - REACH_H, p.y - REACH_V, p.z - REACH_H), Vec3::new(p.x + REACH_H, p.y + REACH_V, p.z + REACH_H))
}
/// True when the raider at `a` is within `PICKUP_REACH2` of the banner at `b` (squared distance).
pub fn picked_up(a: Vec3, b: Vec3) -> bool {
    (a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2) <= PICKUP_REACH2
}
