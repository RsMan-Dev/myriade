//! The fish's swim goal: when it fires, and which spot of the water it keeps.

/// The fire of the swim goal: it ends when the fish is not walking and its active window is over.
pub fn fired_idle(walking: bool, now: u64, active: u64) -> bool {
    !(walking || now >= active)
}

/// One try of the spot pick: the spot is kept when it is pathfindable in water, or after 10 tries.
pub fn keeps_spot(pathfindable_water: bool, count: i32) -> bool {
    pathfindable_water || count >= 10
}
