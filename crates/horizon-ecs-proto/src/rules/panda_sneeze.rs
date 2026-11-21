//! Calculations of the cub's sneeze (`PandaSneezeGoal`, `mobs/panda.rs:903-928`): the deadline of the next ask.

/// The deadline of the next ask: `k` asks of two ticks each, from `now`.
pub fn sneeze_at(now: u64, k: i32) -> u64 {
    now + 2 * k as u64
}
