use horizon_core::geometry::Vec3;
use horizon_core::random::{LegacyRandom, RandomSource};

/// `TICKS_BEFORE_ABOUT_TO_SPAWN_SOUND`.
pub const ABOUT_TO_SPAWN: u32 = 36;
/// The particles are drawn on the game ticks that are multiples of this (`getGameTime() % 5 == 0`).
pub const PARTICLES_EVERY: u64 = 5;

/// The delay after the spawner's start at which its warning sound plays, if the drop is further than that away.
pub fn about_to_spawn_delay(after_ticks: u32) -> Option<i32> {
    (after_ticks > ABOUT_TO_SPAWN).then(|| (after_ticks - ABOUT_TO_SPAWN) as i32)
}

/// The delay after the spawner's start at which it drops its item: at least one tick.
pub fn drop_delay(after_ticks: u32) -> i32 {
    after_ticks.max(1) as i32
}

/// `Projectile.shoot` towards `Direction.DOWN`: the unit vector, plus a triangle spread per axis (`0.0172275 * uncertainty`), all
/// times the power. Draws in the order x, y, z, two per axis, each triangle being the difference of two draws.
pub fn shoot_down(rng: &mut LegacyRandom, power: f32, uncertainty: f32) -> Vec3 {
    let spread = 0.017_227_5 * f64::from(uncertainty);
    let power = f64::from(power);
    let x = (spread * (rng.next_double() - rng.next_double())) * power;
    let y = (-1.0 + spread * (rng.next_double() - rng.next_double())) * power;
    let z = (spread * (rng.next_double() - rng.next_double())) * power;
    Vec3::new(x, y, z)
}

/// The number of particles of a wake-up at the spawner's own tick `local`.
pub fn particle_count(local: u64) -> u32 {
    1 + (local % 3) as u32
}

/// The wait to the next particle wake-up from the game tick `now`: the next game tick that is a multiple of `PARTICLES_EVERY`.
pub fn particle_wait(now: u64) -> u64 {
    PARTICLES_EVERY - now % PARTICLES_EVERY
}
