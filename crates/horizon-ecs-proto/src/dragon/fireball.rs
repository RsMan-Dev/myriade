//! The dragon's fireball (`DragonFireball`; Horizon `spawn_dragon_fireball` D:3086-3112, `dragon_fireball_on_hit` D:3116-3154,
//! `fireball_cloud_origin` D:99-114). Shot at the target, it flies as an `AbstractHurtingProjectile` (the projectile socle's
//! `Order::Hurting`, `crate::projectile_more::DRAGON_FIREBALL`), and where it ends it leaves a breath cloud at the first creature near it.
use crate::area;
use crate::dragon::breath::CloudSpec;
use crate::goals::Events;
use crate::projectile::launch;
use crate::rules::dragon_fireball as rule;
use crate::targeting::{self, Who};
use bevy_ecs::prelude::*;
use horizon_core::geometry::{Aabb, Vec3};

pub use crate::rules::dragon_fireball::{burst_origin, normalize, velocity, SPEED};

pub const SHOT_SOUND: &str = "entity.ender_dragon.shoot";
/// The cloud the burst leaves (jar `DragonFireball.onHit`): radius 3 growing to 7 over 600 ticks, instant damage of amplifier 1.
pub const CLOUD: CloudSpec = CloudSpec { radius: 3.0, grown: 7.0, duration: 600, amplifier: 1 };

/// Shoots a fireball from `at` toward `dir`, owned by `owner`, and writes its sound.
pub fn shoot(w: &mut World, owner: Who, at: Vec3, dir: Vec3) -> Entity {
    w.resource_mut::<Events>().host.sound(SHOT_SOUND, [at.x, at.y, at.z], 10.0, 1.0);
    launch(w, crate::projectile_more::DRAGON_FIREBALL, at, velocity(dir), Some(owner))
}

/// `DragonFireball.onHit` where it ends at `at` (its position, the bottom of its box): spawns the breath cloud at `burst_origin`.
/// The owner-hit rule (`ownedBy`) and the choice of the hit are the projectile's, not here. Not done: level event 2006.
pub fn burst(w: &mut World, at: Vec3) -> Entity {
    let feet = [at.x, at.y, at.z];
    let area: Aabb = rule::search_box(at);
    let mut targets: Vec<(Aabb, Vec3)> = Vec::new();
    for id in area::players_in(w, area) {
        if let Some(i) = targeting::info(w, Who::Player(id)) {
            targets.push((i.bounds, i.pos));
        }
    }
    for m in area::creatures_in(w, area, Entity::PLACEHOLDER) {
        if let Some(i) = targeting::info(w, Who::Entity(m)) {
            targets.push((i.bounds, i.pos));
        }
    }
    let origin = burst_origin(feet, &targets);
    crate::breath_cloud::spawn(w, CLOUD, origin)
}
