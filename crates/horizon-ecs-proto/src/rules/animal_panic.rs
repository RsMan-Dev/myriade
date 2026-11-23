//! Panic of the animals with a brain (camel, frog, nautilus): the start window, the rider that holds it back, its duration,
//! and the water it runs to while burning (`myriade-engine/src/mobs/animal_panic.rs:11-160`).

/// Which rider holds the panic back: a player (`m.rider`, the camel), or any rider (the husk: `extra.rider` too).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Riders {
    Player,
    Any,
}

/// A hurt by a panic cause, `ticks_since` ticks ago at most `window`: the panic can start.
pub fn in_window(panic: bool, now: u64, at: u64, window: u64) -> bool {
    panic && now.saturating_sub(at) <= window
}

/// The rider holds the panic back: `rider` is None without a rider, else whether the rider is a player.
pub fn held(riders: Riders, rider: Option<bool>) -> bool {
    matches!((riders, rider), (Riders::Player, Some(true)) | (Riders::Any, Some(_)))
}

/// The duration of the panic: `min` plus the draw `r` of `nextInt(spread)`.
pub fn duration(min: u64, r: i32) -> u64 {
    min + r as u64
}

/// The tick the panic ends, `dur` ticks after `now`.
pub fn ends_at(now: u64, dur: u64) -> u64 {
    now + dur
}

/// The animal looks for water while it is burning past `now`.
pub fn wants_water(burning_until: Option<i64>, now: u64) -> bool {
    burning_until.is_some_and(|until| until > now as i64)
}

/// A body that rounds up to two blocks wide (`width` in blocks).
pub fn is_wide(width: f64) -> bool {
    width.ceil() == 2.0
}

/// The block the position is in.
pub fn block_of(x: f64, y: f64, z: f64) -> [i32; 3] {
    [x.floor() as i32, y.floor() as i32, z.floor() as i32]
}

/// A water block is a spot when the body fits: a wide body also needs the water to its south, east and south-east.
pub fn water_spot_ok(wide: bool, p: [i32; 3], water: impl Fn(i32, i32, i32) -> bool) -> bool {
    water(p[0], p[1], p[2]) && (!wide || (water(p[0], p[1], p[2] + 1) && water(p[0] + 1, p[1], p[2]) && water(p[0] + 1, p[1], p[2] + 1)))
}

/// The centre of a spot block, at the water's level.
pub fn spot_centre(p: [i32; 3]) -> [f64; 3] {
    [f64::from(p[0]) + 0.5, f64::from(p[1]), f64::from(p[2]) + 0.5]
}
