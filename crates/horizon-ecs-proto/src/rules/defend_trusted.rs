//! Defence of a trusted entity: the freshness of a hit, the reach of the attacker and the target's memory in ticks.

/// A hit taken at tick `at` is still avenged at tick `now`: within 100 ticks.
pub fn hit_is_fresh(now: u64, at: u64) -> bool {
    now.saturating_sub(at) <= 100
}

/// The attacker is within `range`: the squared offset is at most range squared.
pub fn within_reach(dx: f64, dy: f64, dz: f64, range: f64) -> bool {
    dx * dx + dy * dy + dz * dz <= range * range
}

/// The `unseenMemoryTicks` of a target as the goal gives it: half of the value, rounded up.
pub fn target_memory(unseen_memory: i32) -> u16 {
    ((unseen_memory + 1) / 2) as u16
}
