/// The box of a body of size `size`: its base box times the size (`AbstractCubeMob.setSize`).
pub fn box_of(base: (f32, f32), size: f32) -> (f32, f32) {
    (base.0 * size, base.1 * size)
}

/// The movement speed of a body of size `size`: `speed_base + speed_per_size * size`, in f32 as vanilla computes it.
pub fn speed_of(speed_base: f64, speed_per_size: f64, size: f32) -> f64 {
    f64::from(speed_base as f32 + speed_per_size as f32 * size)
}

/// The health of a body of size `size`: `health_per_sq` times the size squared.
pub fn health_of(health_per_sq: f32, size: f32) -> f32 {
    health_per_sq * size * size
}

/// The attack damage of a body of size `size`: `damage_per_size` times the size.
pub fn damage_of(damage_per_size: f32, size: f32) -> f32 {
    damage_per_size * size
}

/// The offset of the `i`th split body from the dead body's centre, in units of half its width: a quarter of its width on each axis.
pub fn split_offset(i: u32) -> (f32, f32) {
    ((i % 2) as f32 - 0.5, (i / 2) as f32 - 0.5)
}
