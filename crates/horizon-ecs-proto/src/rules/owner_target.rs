//! Calculations of the pet's owner-target goals (`OwnerHurtByTargetGoal`, `OwnerHurtTargetGoal`): how recent the owner's
//! record of being hurt must be to count.

/// The record of the owner being hurt counts only within this many ticks (`getLastHurtByMob`).
pub const HURT_MEMORY: u64 = 100;

/// True when the owner was hurt at `at` and that is within `HURT_MEMORY` ticks of `now`.
pub fn recent_hurt(now: u64, at: u64) -> bool {
    now.saturating_sub(at) <= HURT_MEMORY
}
