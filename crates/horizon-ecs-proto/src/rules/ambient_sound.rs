//! Ambient sounds of the piglin (`mobs/piglin.rs:478-512`): the delay before the next sound and the sound picked.

/// The ticks before the next sound: `min` plus the draw `r` taken modulo `max - min + 1` (drawn in `min..=max`).
pub fn delay(min: u32, max: u32, r: u32) -> i32 {
    (min + r % (max - min + 1)) as i32
}

/// The index of the sound played: the draw `r` taken modulo `len` (`len` > 0).
pub fn pick(r: u32, len: usize) -> usize {
    (r as usize) % len
}
