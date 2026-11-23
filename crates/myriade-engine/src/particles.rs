//! What the server sends about particles: the options a mod or a mob asks
//! for, their types, the firework explosions, the bursts a mob's event
//! carries, and the crumbs an item use throws (positions only).
pub use myriade_data::particle_type::ParticleType;
pub use horizon_items::firework::{DYE_FIREWORK_COLORS, FireworkExplosion, FireworkShape};

pub use myriade_level::particle_options::ParticleOptions;

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
    /// `Ocelot.spawnTrustingParticles`, `Animal`'s entity event 18.
    pub const TAMING: Self = Self { count: 7, speed: Some(0.02), y: 0.5 };
    /// `Allay`'s entity event 18: three `spawnHeartParticle`.
    pub const ALLAY: Self = Self { count: 3, speed: Some(0.02), y: 0.5 };
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
/// The particle bursts entity events show on the client: the particle and
/// burst of `handleEntityEvent(event)` for the entity type `entity`.
pub fn entity_event_burst(entity: &str, event: u8) -> Option<(&'static str, Burst)> {
    let entity = entity.strip_prefix("minecraft:").unwrap_or(entity);
    let tamable = matches!(
        entity,
        "wolf" | "cat" | "parrot" | "nautilus" | "zombie_nautilus"
    );
    let horse = matches!(
        entity,
        "horse"
            | "donkey"
            | "mule"
            | "llama"
            | "trader_llama"
            | "camel"
            | "camel_husk"
            | "skeleton_horse"
            | "zombie_horse"
    );
    Some(match (entity, event) {
        ("villager", 12) => ("heart", Burst::VILLAGER),
        ("villager", 13) => ("angry_villager", Burst::VILLAGER),
        ("villager", 14) => ("happy_villager", Burst::VILLAGER),
        ("villager", 42) => ("splash", Burst::VILLAGER),
        ("allay", 18) => ("heart", Burst::ALLAY),
        ("dolphin", 38) => ("happy_villager", Burst::DOLPHIN),
        ("ocelot", 41) => ("heart", Burst::TAMING),
        ("ocelot", 40) => ("smoke", Burst::TAMING),
        (_, 7) if tamable || horse => ("heart", Burst::TAMING),
        (_, 6) if tamable || horse => ("smoke", Burst::TAMING),
        (_, 18) if !matches!(entity, "villager" | "allay") => ("heart", Burst::TAMING),
        _ => return None,
    })
}

/// `ExplosionParticleInfo`: one block particle of an explosion, with how much of the distance
/// drawn for it it travels, what its speed is multiplied by, and its weight in the pick.
#[derive(Clone, Debug, PartialEq)]
pub struct ExplosionParticle {
    pub options: ParticleOptions,
    pub scaling: f32,
    pub speed: f32,
    pub weight: u32,
}
impl ExplosionParticle {
    /// `Level.DEFAULT_EXPLOSION_BLOCK_PARTICLES`: poof (scaling 0.5) and smoke, one each.
    pub fn defaults() -> Vec<Self> {
        let entry = |ty, scaling| Self { options: ParticleOptions::simple(ty), scaling, speed: 1.0, weight: 1 };
        vec![entry(ParticleType::Poof, 0.5), entry(ParticleType::Smoke, 1.0)]
    }
}
