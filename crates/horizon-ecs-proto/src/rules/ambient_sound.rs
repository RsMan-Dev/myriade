//! Ambient sounds of the piglin (`mobs/piglin.rs:478-512`): the delay before the next sound and the sound picked.

/// The ticks before the next sound: `min` plus the draw `r` taken modulo `max - min + 1` (drawn in `min..=max`).
pub fn delay(min: u32, max: u32, r: u32) -> i32 {
    (min + r % (max - min + 1)) as i32
}

/// `Mob.baseTick`: the counter restarts at `-interval` and rises by one each tick, and a sound plays on the tick where the draw `r`
/// (`nextInt(1000)`) is below it. Drawn once: the sound falls `r + interval + 2` ticks after the last one.
pub fn vanilla_delay(interval: u32, r: u32) -> i32 {
    (r + interval + 2) as i32
}

/// The index of the sound played: the draw `r` taken modulo `len` (`len` > 0).
pub fn pick(r: u32, len: usize) -> usize {
    (r as usize) % len
}

/// `Mob.baseTick` as vanilla runs it: after a sound the counter is `-interval`; each tick draws `nextInt(1000)` and plays when the draw
/// is below the counter, then the counter rises. Returns the ticks until the play (`draw` yields one `nextInt(1000)` per tick).
pub fn base_tick_delay(interval: u32, mut draw: impl FnMut() -> u32) -> i32 {
    let mut k = 1_i32;
    while (draw() as i32) >= k - 1 - interval as i32 {
        k += 1;
    }
    k
}
