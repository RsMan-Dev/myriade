//! The sheep's grazing: the age a baby reaches when it eats.

/// `AgeableMob.ageUp(60)` for a baby that ate: its growth moves 1200 ticks closer, and never before the next tick.
pub fn age_up(grow_at: u64, now: u64) -> u64 {
    grow_at.saturating_sub(1200).max(now + 1)
}
