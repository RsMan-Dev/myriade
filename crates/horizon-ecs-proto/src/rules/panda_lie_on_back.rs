//! Calculations of the lazy panda's lying on its back (`PandaLieOnBackGoal`, `mobs/panda.rs:879-902`): the deadline of the
//! next start ask after the cooldown, and the end of the rest.

/// The deadline of the next start ask: after the cooldown ends (`until`, at least `now`), plus two ticks per ask (`asks`).
pub fn lie_start_at(until: u64, now: u64, asks: i32) -> u64 {
    (until + 1).max(now) + 2 * asks as u64
}
/// The rest ends when the panda is in water, or when no ask of the rest is left (`asks_left` already counted down).
pub fn rest_ends(wet: bool, asks_left: i32) -> bool {
    wet || asks_left <= 0
}
