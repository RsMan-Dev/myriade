//! The numbers of a hostile monster: its melee reach, cooldowns and repath, its health after a hit, and its chance to burn in daylight.
use myriade_engine_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;

/// The damage of a monster's blow: its own, or 1 when it has none.
pub fn attack_damage(damage: f32) -> f32 {
    if damage <= 0.0 { 1.0 } else { damage }
}

/// `IronGolem.doHurtTarget`: half the attack damage plus a draw below its integer part (`draw(bound)`), or the damage itself below 1.
pub fn spread_damage(damage: f32, draw: impl FnOnce(i32) -> i32) -> f32 {
    let whole = damage as i32;
    if whole > 0 { damage / 2.0 + draw(whole) as f32 } else { damage }
}

/// The reach of a melee monster: `sqrt(2.04) - 0.6` blocks, the box widened on x and z.
pub fn melee_reach() -> f64 {
    2.04_f32.sqrt() as f64 - 0.6
}

/// The wait of the melee check: the next 20-tick grid point after `now`, the grid starting at `phase`.
pub fn melee_wait(now: u64, phase: u64) -> i32 {
    20 - (now.saturating_sub(phase) % 20) as i32
}

/// The wait before the melee check runs again: at once when the goal follows its target unseen or is late, else the grid's 20 ticks.
pub fn melee_retry_wait(now: u64, phase: u64, follow_unseen: bool) -> i32 {
    if follow_unseen || now >= phase + 20 { 1 } else { (phase + 20 - now) as i32 }
}

/// Whether a restart at `now` finds the cooldown run out: the goal stopped more than 3 ticks before.
pub fn cooldown_expired_at_restart(now: u64, stopped_at: u64) -> bool {
    now.saturating_sub(stopped_at) > 3
}

/// Whether the target has been out of reach too long: no path has reached it since `since` (0: never) for more than `allowed` ticks.
pub fn unreachable_too_long(now: u64, since: u64, allowed: i32) -> bool {
    since != 0 && now.saturating_sub(since) > allowed as u64
}

/// The extra ticks before the next path ask when the target is far: 10 beyond 32 blocks, 5 beyond 16 (squared distance `d`).
pub fn far_repath_extra(d: f64) -> i32 {
    if d > 1024.0 {
        10
    } else if d > 256.0 {
        5
    } else {
        0
    }
}

/// The health after a hit of `amount`; a monster kept alive (`immortal`) never drops below 1.
pub fn health_after(health: f32, amount: f32, immortal: bool) -> f32 {
    let h = health - amount;
    if immortal { h.max(1.0) } else { h }
}

/// The block the eyes of a body at `p` (feet) with `eye` height are in.
pub fn eye_block(p: Vec3, eye: f64) -> (i32, i32, i32) {
    (p.x.floor() as i32, (p.y + eye).floor() as i32, p.z.floor() as i32)
}

/// The probability per tick of a burn at this block, 0 when the light is too low or the sky is not seen.
pub fn burn_chance(mw: &MobWorld, day: bool, b: (i32, i32, i32)) -> f64 {
    if !day || !mw.can_see_sky(b.0, b.1, b.2) {
        return 0.0;
    }
    let br = f64::from(mw.brightness(b.0, b.1, b.2));
    if br > 0.5 { ((br - 0.4) * 2.0 / 30.0).min(1.0) } else { 0.0 }
}

/// The number of failed draws before the first success of a 1-in-`1/p` draw, from the uniform draw `u` of the tick.
pub fn failures_before(u: f64, p: f64) -> u64 {
    let u = u.max(1.0e-12);
    (u.ln() / (1.0 - p).ln()).floor().min(4000.0).max(0.0) as u64
}
