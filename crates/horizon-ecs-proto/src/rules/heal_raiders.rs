//! Timer of the raider healing (`HEAL_RAIDERS`, the witch's pause): the healer waits 200 ticks after a heal.

/// Ticks the healer waits after a heal.
const WAIT: u64 = 200;

/// The tick the healer may heal again, given the tick of the heal.
pub fn wait_until(now: u64) -> u64 {
    now + WAIT
}

/// The healer is still waiting at `now`, given the tick it waits until.
pub fn still_waiting(until: u64, now: u64) -> bool {
    until > now
}
