//! The rules of the dragon's phases: the takeoff's node, the strafe's hover height and its aim, the fire's line, the charge's expiry.
use horizon_core::geometry::Vec3;

/// The node the takeoff heads for, from the closest node to the look point: the inner ring while crystals stand, else the outer one.
pub fn takeoff_node(closest: usize, crystals_alive: u16) -> usize {
    let t0 = closest as i32;
    let t = if crystals_alive > 0 { t0.rem_euclid(12) } else { ((t0 - 12) & 7) + 12 };
    t as usize
}

/// The strafe's aim point over the player `pp`: its head height raised by the distance from `here` (`ho`, at most 10 blocks).
pub fn strafe_target(pp: Vec3, here: Vec3) -> Vec3 {
    let ho = (0.4 + (pp.x - here.x).hypot(pp.z - here.z) / 80.0 - 1.0).min(10.0);
    Vec3::new(pp.x, pp.y + ho, pp.z)
}

/// The angle in degrees between the facing `dir` and the aim `aim` (both unit `(x, z)`), with the half degree of slack.
pub fn aim_angle(dir: (f64, f64), aim: (f64, f64)) -> f64 {
    (dir.0 * aim.0 + dir.1 * aim.1).clamp(-1.0, 1.0).acos().to_degrees() + 0.5
}

/// Whether the strafe fires: the fifth shot seen in line, and the aim within 10 degrees.
pub fn fires(charge: i32, angle: f64) -> bool {
    charge >= 5 && (0.0..10.0).contains(&angle)
}

/// The fireball's start (from the head, lowered by the view and raised a block) and its direction towards the player's feet.
pub fn fire_line(head: Vec3, view: Vec3, pp: Vec3) -> (Vec3, Vec3) {
    let start = Vec3::new(head.x - view.x, head.y + 1.0, head.z - view.z);
    (start, Vec3::new(pp.x - start.x, pp.y + 0.9 - start.y, pp.z - start.z))
}

/// The charge gives up after 9 ticks from its start.
pub fn charge_expired(now: u64, since: u64) -> bool {
    now.saturating_sub(since) >= 9
}
