//! Calculs de la régénération (`WitherBoss.aiStep`) : la vie après un soin, et la date du prochain soin après un coup.

/// The health after a heal of `amount`, capped at `max` (a heal at full health does nothing).
pub fn healed(health: f32, amount: f32, max: f32) -> f32 {
    (health + amount).min(max)
}

/// The next heal date of a sleeping regeneration woken at `now`: the first date of its phase (`due` + k * `step`) not before `now`.
pub fn realigned_due(due: u64, now: u64, step: u64) -> u64 {
    due + now.saturating_sub(due) / step * step + if now.saturating_sub(due) % step == 0 { 0 } else { step }
}
