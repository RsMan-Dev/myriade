//! The rules of a lingering cloud: its growth and its end, the wait and the duration, the reach test of a victim, the amount of an
//! instantaneous brew and the scale of a timed one.

/// The radius after `by` more: the clamped radius and whether the cloud lives (it dies under 0.5 blocks).
pub fn grow(radius: f32, by: f32) -> (f32, bool) {
    let r = radius + by;
    if r < 0.5 { (r, false) } else { (r.clamp(0.0, 32.0), true) }
}

/// Whether the cloud is over at `age`: its duration (-1: none) counted from the end of its wait.
pub fn over(age: i32, wait: i32, duration: i32) -> bool {
    duration != -1 && age.wrapping_sub(wait) >= duration
}

/// The duration after a victim's dose adds `on_use` to it, and whether it still lives (above 0 ticks).
pub fn extend(duration: i32, on_use: i32) -> (i32, bool) {
    let d = duration.wrapping_add(on_use);
    (d, d > 0)
}

/// Whether `(dx, dz)` from the cloud's floor lies outside its circle of `radius`.
pub fn outside(dx: f64, dz: f64, radius: f32) -> bool {
    dx * dx + dz * dz > f64::from(radius * radius)
}

/// The amount of an instantaneous brew of `base` at `level` at half strength (`base << level`, as `instant` of the client).
pub fn instant(base: i32, level: i32) -> f32 {
    (0.5 * f64::from(base.wrapping_shl(level as u32)) + 0.5) as i32 as f32
}

/// `MobEffectInstance.withScaledDuration`: -1 and 0 are kept, else the duration times the scale, floored, at least one tick.
pub fn scaled(ticks: i32, scale: f32) -> i32 {
    if ticks == -1 || ticks == 0 { ticks } else { ((ticks as f32 * scale).floor() as i32).max(1) }
}
