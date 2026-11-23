use crate::projectiles::{Projectile, ProjectileKind};
use horizon_core::geometry::trig::Mth;
use horizon_core::{
    geometry::{Aabb, Vec3},
    random::RandomSource,
};
use std::f32::consts::PI;


/// `Blaze.BlazeAttackGoal`: melee up close, otherwise a 3-second charge and
/// three small fireballs, then a five-second rest.
#[derive(Clone, Debug, Default)]
struct BlazeAttack {
    step: i32,
    time: i32,
    last_seen: i32,
}

/// `Ghast.explosionPower`'s `Extra::ints` slot.
const EXPLOSION_POWER: usize = 0;

/// `(180.0F / (float)Math.PI)` of `GhastLookGoal.tick`.
fn ghast_deg() -> f32 {
    180.0_f32 / std::f32::consts::PI
}
/// `Ghast.RandomFloatAroundGoal`.
#[derive(Clone, Debug)]
struct RandomFloatAround;
/// `Ghast.GhastLookGoal`: face the motion, or the target within 64 blocks.
#[derive(Clone, Debug)]
struct GhastLook;
/// `Ghast.GhastShootFireballGoal`: a second of charging, then a fireball.
#[derive(Clone, Debug)]
struct GhastShoot {
    charge: i32,
}
