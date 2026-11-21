//! What the server sends about particles: the options a mod or a mob asks
//! for, their types, the firework explosions, the bursts a mob's event
//! carries, and the crumbs an item use throws (positions only).
pub use horizon_client_core::particle_type::ParticleType;
pub use horizon_items::firework::{DYE_FIREWORK_COLORS, FireworkExplosion, FireworkShape};

pub use horizon_client_render::particle_options::ParticleOptions;

use horizon_core::{
    geometry::{Vec3, trig},
    random::RandomSource,
};

/// A burst of particles about an entity's box (`around_self`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Burst {
    pub count: u32,
    /// Scale of the Gaussian speeds; `None` spawns them at rest without drawing any.
    pub speed: Option<f64>,
    /// Added to `getRandomY()`.
    pub y: f64,
}
impl Burst {
    /// `AbstractVillager.addParticlesAroundSelf`.
    pub const VILLAGER: Self = Self {
        count: 5,
        speed: Some(0.02),
        y: 1.0,
    };
    /// `Dolphin.addParticlesAroundSelf`.
    pub const DOLPHIN: Self = Self {
        count: 7,
        speed: Some(0.01),
        y: 0.2,
    };
}

/// `LivingEntity.spawnItemParticles(stack, count)` (eating, drinking, an item
/// breaking): `count` crumbs in front of the eyes, thrown up and forwards,
/// both turned by the pitch and yaw in degrees.
pub fn item_use_particles(
    random: &mut impl RandomSource,
    pitch: f32,
    yaw: f32,
    position: [f64; 3],
    eye_y: f64,
    count: u32,
    mut emit: impl FnMut([f64; 3], [f64; 3]),
) {
    let (x_rot, y_rot) = (-pitch * trig::DEG_TO_RAD, -yaw * trig::DEG_TO_RAD);
    for _ in 0..count {
        let d = Vec3::new(
            (f64::from(random.next_float()) - 0.5) * 0.1,
            f64::from(random.next_float()) * 0.1 + 0.1,
            0.0,
        )
        .rotate_x(x_rot)
        .rotate_y(y_rot);
        let y = f64::from(-random.next_float()) * 0.6 - 0.3;
        let p = Vec3::new((f64::from(random.next_float()) - 0.5) * 0.3, y, 0.6)
            .rotate_x(x_rot)
            .rotate_y(y_rot);
        emit(
            [p.x + position[0], p.y + eye_y, p.z + position[2]],
            [d.x, d.y + 0.05, d.z],
        );
    }
}
