//! Values of the happy ghast's heal (`happy_ghast.rs` `ai_step`, 604-623).

/// The ticks until the next heal: the wet interval in clouds or precipitation, else the dry one.
pub fn heal_interval(wet: bool, dry: i32, wet_ticks: i32) -> i32 {
    if wet { wet_ticks } else { dry }
}

/// The health after one heal, capped at the maximum.
pub fn healed(health: f32, amount: f32, max: f32) -> f32 {
    (health + amount).min(max)
}
