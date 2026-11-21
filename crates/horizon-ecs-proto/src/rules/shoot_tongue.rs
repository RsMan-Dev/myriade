use horizon_core::geometry::Vec3;

/// The velocity that draws the prey in: 0.75 blocks per tick toward the frog at `me`.
pub fn pull_toward(me: Vec3, prey: Vec3) -> Vec3 {
    Vec3::new(me.x - prey.x, me.y - prey.y, me.z - prey.z).normalize().scale(0.75)
}

/// The prey is within the eating distance; `d2` is the squared distance.
pub fn in_eating_reach(d2: f64, eating_distance: f64) -> bool {
    d2 < eating_distance * eating_distance
}

/// The prey is within the detection distance; `d2` is the squared distance.
pub fn in_detection(d2: f64, detection: f64) -> bool {
    d2 < detection * detection
}

/// The catch lasts 6 ticks: over that, the prey is eaten.
pub fn catch_done(eat_timer: i8) -> bool {
    eat_timer > 6
}

/// The meal lasts 10 ticks.
pub fn eat_done(eat_timer: i8) -> bool {
    eat_timer >= 10
}
