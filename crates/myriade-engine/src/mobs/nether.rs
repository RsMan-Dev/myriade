//! Nether flyers: blazes (hovering, three-fireball volleys, the spinning
//! rods) and ghasts (floating about, charging and shooting large fireballs).
use super::{
    MobEvent, MobWorld, Who,
    anim::{Poses, State},
    control::{MoveKind, Operation},
    goals::{
        Cx, Goal, HurtByTarget, LOOK, LookAtPlayer, MOVE, NearestTarget, RandomLookAround, Stroll,
        goal_clone,
    },
};
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
impl Goal for BlazeAttack {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK
    }
    fn every_tick(&self) -> bool {
        true
    }
}
/// `BlazeModel.setupAnim`: three rings of rods circling and bobbing.
pub fn blaze_anim(s: &State, p: &mut Poses) {
    let age = s.age;
    let mut angle = age * PI * -0.1;
    for i in 0..4 {
        let part = p.get(&format!("part{i}"));
        part[1] = -2.0 + ((i * 2) as f32 + age).mul_add(0.25, 0.0).mcos();
        part[0] = angle.mcos() * 9.0;
        part[2] = angle.msin() * 9.0;
        angle += PI / 2.0;
    }
    angle = PI / 4.0 + age * PI * 0.03;
    for i in 4..8 {
        let part = p.get(&format!("part{i}"));
        part[1] = 2.0 + (((i * 2) as f32 + age) * 0.25).mcos();
        part[0] = angle.mcos() * 7.0;
        part[2] = angle.msin() * 7.0;
        angle += PI / 2.0;
    }
    angle = 0.471_238_94 + age * PI * -0.05;
    for i in 8..12 {
        let part = p.get(&format!("part{i}"));
        part[1] = 11.0 + ((i as f32 * 1.5 + age) * 0.5).mcos();
        part[0] = angle.mcos() * 5.0;
        part[2] = angle.msin() * 5.0;
        angle += PI / 2.0;
    }
    p.yrot("head", s.y_rot.to_radians());
    p.xrot("head", s.x_rot.to_radians());
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
impl Goal for RandomFloatAround {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE
    }
}
/// `Ghast.GhastLookGoal`: face the motion, or the target within 64 blocks.
#[derive(Clone, Debug)]
struct GhastLook;
impl Goal for GhastLook {
    goal_clone!();
    fn flags(&self) -> u8 {
        LOOK
    }
    fn every_tick(&self) -> bool {
        true
    }
}
/// `Ghast.GhastShootFireballGoal`: a second of charging, then a fireball.
#[derive(Clone, Debug)]
struct GhastShoot {
    charge: i32,
}
impl Goal for GhastShoot {
    goal_clone!();
    fn flags(&self) -> u8 {
        0
    }
    fn every_tick(&self) -> bool {
        true
    }
}
/// `GhastModel.animateTentacles`.
pub fn ghast_anim(s: &State, p: &mut Poses) {
    for i in 0..9 {
        let name = format!("tentacle{i}");
        if p.has(&name) {
            p.xrot(&name, 0.2 * (s.age * 0.3 + i as f32).msin() + 0.4);
        }
    }
}
