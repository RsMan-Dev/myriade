//! The rules of the status effects: the period of a periodic effect, its first act and its end, the replacement of an effect by a new
//! one, the catch-up of a deadline, and the splash of a thrown potion (its reach, its scale, its amounts).

/// The period of a periodic effect of base `base` at level `amp` (`base >> amp`, at least one tick).
pub fn period(base: i32, amp: i32) -> i32 {
    base.wrapping_shr(amp as u32).max(1)
}

/// The last tick of an effect of `ticks` that starts after `first`: it acts on it and is gone after it. With no duration
/// (`MobEffectInstance.hasRemainingDuration` false) the last tick is the one before `first`: it never acts.
pub fn ends_at(first: u64, ticks: i32) -> u64 {
    (first + ticks.max(0) as u64).saturating_sub(1)
}

/// The first tick a periodic effect of `period` acts on: the remaining time is a multiple of the period.
pub fn first_act(first: u64, ticks: i32, period: i32) -> u64 {
    first + (ticks.max(0) as u64) % period as u64
}

/// Whether a new effect (`amp`, `ends`) replaces the one it meets (`old_amp`, `old_ends`): a higher level, or the same and longer.
pub fn replaces(amp: i32, ends: u64, old_amp: i32, old_ends: u64) -> bool {
    amp > old_amp || amp == old_amp && ends > old_ends
}

/// Moves a periodic deadline `next` past `now` by `period` steps while it stays within `ends`; the flag tells whether one of its acts
/// falls on `now`.
pub fn settle(mut next: u64, ends: u64, now: u64, period: u64) -> (u64, bool) {
    let mut hit = false;
    while next <= now && next <= ends {
        hit |= next == now;
        next += period;
    }
    (next, hit)
}

/// Whether a splash reaches a creature at squared distance `d2`: within 4 blocks, or the creature hit itself.
pub fn splash_reaches(d2: f64, direct: bool) -> bool {
    !(d2 >= 16.0 && !direct)
}

/// The share of a splash at squared distance `d2`: whole for the creature hit, else `1 - distance / 4`.
pub fn splash_scale(d2: f64, direct: bool) -> f64 {
    if direct { 1.0 } else { 1.0 - d2.sqrt() / 4.0 }
}

/// The length of a timed effect at `scale`: `(int)(scale * ticks * 0.75 + 0.5)`.
pub fn timed_ticks(scale: f64, ticks: i32) -> i32 {
    (scale * f64::from(ticks) * 0.75 + 0.5) as i32
}

/// The amount of an instantaneous brew of `base` at `level` at `scale`: `base << level`, scaled and floored.
pub fn harm_amount(scale: f64, base: i32, level: i32) -> f32 {
    (scale * f64::from(base.wrapping_shl(level as u32)) + 0.5).floor() as f32
}

/// Whether a brew hurts: harming hurts, healing heals, and an undead creature has them swapped.
pub fn harms(is_harm: bool, undead: bool) -> bool {
    is_harm != undead
}
