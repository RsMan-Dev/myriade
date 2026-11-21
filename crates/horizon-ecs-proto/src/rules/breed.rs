//! Calculations of the breeding goal: the selector's ticks, the love counted in them, the birth, the end of the love, the
//! deadline a baby inherits, and the orbs of a birth.

/// Whether tick `t` is one of the selector's ticks (every other one, shifted by the goal's `phase`).
pub fn is_selector_tick(t: u64, phase: u64) -> bool {
    (t + phase) % 2 == 0
}
/// `BreedGoal.loveTime`: the selector ticks from the first one after `since` up to `now`.
pub fn love_ticks(now: u64, since: u64, phase: u64) -> u64 {
    let first = if is_selector_tick(since, phase) { since } else { since + 1 };
    if now >= first { (now - first) / 2 + 1 } else { 0 }
}
/// The birth: on a selector tick, the love has reached half the delay and the partner is within 3 blocks (`dist2`, squared).
pub fn birth_due(selector: bool, love_time: u64, delay: u64, dist2: f64) -> bool {
    selector && love_time >= delay / 2 && dist2 < 9.0
}
/// The goal gives up once the love has counted more than the delay.
pub fn gives_up(love_time: u64, delay: u64) -> bool {
    love_time > delay
}
/// The deadline a baby inherits from its parent's (`active`, in the parent's time `now`), in the baby's time `baby_now`: an
/// endless deadline stays endless.
pub fn inherited_deadline(active: u64, now: u64, baby_now: u64) -> u64 {
    if active == u64::MAX { active } else { baby_now + active.saturating_sub(now) }
}
/// `finalizeSpawnChildFromBreeding`: the orbs of a birth, `1 + draw` for the draw `draw` of `nextInt(7)`.
pub fn birth_orbs(draw: i32) -> u32 {
    (1 + draw) as u32
}
