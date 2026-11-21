//! Calculations of the panda's sitting (`PandaSitGoal`, `mobs/panda.rs:820-876`): the cool-down after a sit, and the retry
//! time of a refused ask.

/// The end of the cool-down after a sit: `draw` is `nextInt(n)`, the wait is `draw + 10` times 20 ticks.
pub fn sit_cooldown_end(now: u64, draw: i32) -> u64 {
    now + (draw + 10) as u64 * 20
}
/// The retry of a refused ask with an item in hand: the cool-down's end if it is ahead, else two ticks on.
pub fn sit_retry_at(cooldown: u64, now: u64) -> u64 {
    if cooldown > now { cooldown } else { now + 2 }
}
