//! The dragon's fireball (`DragonFireball`; Horizon `spawn_dragon_fireball` D:3086-3112, `dragon_fireball_on_hit` D:3116-3154,
//! `fireball_cloud_origin` D:99-114). Shot at the target, it flies as an `AbstractHurtingProjectile` (the projectile socle's
//! `Order::Hurting`, `Flight::dragon_fireball()`), and where it ends it leaves a breath cloud at the first creature near it.
use crate::area;
use crate::dragon::breath::{self, Cloud, CloudSpec};
use crate::goals::Events;
use crate::projectile::{launch, Flight};
use crate::targeting::{self, Who};
use bevy_ecs::prelude::*;
use horizon_core::geometry::{Aabb, Vec3};

/// The launch speed, which is also the acceleration of each tick (`accelerationPower` 0.1, D:3109-3110).
pub const SPEED: f64 = 0.1;
pub const SHOT_SOUND: &str = "entity.ender_dragon.shoot";
/// The cloud the burst leaves (jar `DragonFireball.onHit`): radius 3 growing to 7 over 600 ticks, instant damage of amplifier 1.
pub const CLOUD: CloudSpec = CloudSpec { radius: 3.0, grown: 7.0, duration: 600, amplifier: 1 };

/// `Vec3.normalize`: a vector shorter than 1e-5 is zero.
pub fn normalize(v: Vec3) -> Vec3 {
    let l = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
    if l < 1.0e-5 {
        Vec3::new(0.0, 0.0, 0.0)
    } else {
        Vec3::new(v.x / l, v.y / l, v.z / l)
    }
}

/// The launch velocity toward `dir` (normalised twice, as Horizon does), at `SPEED`.
pub fn velocity(dir: Vec3) -> Vec3 {
    let n = normalize(normalize(dir));
    Vec3::new(n.x * SPEED, n.y * SPEED, n.z * SPEED)
}

/// Shoots a fireball from `at` toward `dir`, owned by `owner`, and writes its sound.
pub fn shoot(w: &mut World, owner: Who, at: Vec3, dir: Vec3) -> Entity {
    w.resource_mut::<Events>().host.sound(SHOT_SOUND, [at.x, at.y, at.z], 10.0, 1.0);
    launch(w, Flight::dragon_fireball(), at, velocity(dir), Some(owner))
}

/// `fireball_cloud_origin` (D:102-114; jar `DragonFireball.onHit`): the first target whose box meets the fireball's box inflated
/// by (4, 2, 4) and whose position lies within 4 blocks of `feet` (the fireball's position) gives the cloud's place; else `feet`.
pub fn burst_origin(feet: [f64; 3], targets: &[(Aabb, Vec3)]) -> [f64; 3] {
    let area = Aabb::new(
        Vec3::new(feet[0] - 4.5, feet[1] - 2.0, feet[2] - 4.5),
        Vec3::new(feet[0] + 4.5, feet[1] + 3.0, feet[2] + 4.5),
    );
    for &(bounds, pos) in targets {
        let (dx, dy, dz) = (pos.x - feet[0], pos.y - feet[1], pos.z - feet[2]);
        if bounds.intersects(area) && dx * dx + dy * dy + dz * dz < 16.0 {
            return [pos.x, pos.y, pos.z];
        }
    }
    feet
}

/// `DragonFireball.onHit` where it ends at `at` (its position, the bottom of its box): spawns the breath cloud at `burst_origin`.
/// The owner-hit rule (`ownedBy`) and the choice of the hit are the projectile's, not here. Not done: level event 2006.
pub fn burst(w: &mut World, at: Vec3) -> Entity {
    let feet = [at.x, at.y, at.z];
    let fireball_box = Aabb::new(Vec3::new(at.x - 0.5, at.y, at.z - 0.5), Vec3::new(at.x + 0.5, at.y + 1.0, at.z + 0.5));
    let area = fireball_box.inflate(Vec3::new(4.0, 2.0, 4.0));
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
    let cloud: Cloud = CLOUD.cloud(origin);
    breath::spawn(w, cloud)
}
