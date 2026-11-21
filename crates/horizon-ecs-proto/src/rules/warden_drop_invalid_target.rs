//! The warden's dig cooldown after it drops a target.

/// The tick the digging may resume: `cooldown` ticks after `now`.
pub fn cooldown_end(now: u64, cooldown: u64) -> u64 {
    now + cooldown
}
