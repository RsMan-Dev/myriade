//! The ends of a shulker bullet (`ShulkerBullet.hurtServer`, `onHitBlock`): the flight is `homing_projectile`'s, the blow's sound and
//! particles are here. The bullet itself is despawned by `projectile::fly` on every hit.
use crate::behaviours::homing_projectile::Homing;
use crate::cow::Pos;
use crate::goals::Events;
use crate::projectile::Stuck;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;

/// A creature hit: the hurt sound, crit particles, then the game event `entity_damage` at the blow.
pub fn hurt(w: &mut World, at: Vec3) {
    let p = [at.x, at.y, at.z];
    let mut ev = w.resource_mut::<Events>();
    ev.host.sound("entity.shulker_bullet.hurt", p, 1.0, 1.0);
    ev.host.particles("minecraft:crit", p, [0.2; 3], 0.0, 15);
    ev.host.game_event("minecraft:entity_damage", p);
}

/// A block hit: the hit sound and two explosion particles.
pub fn ends(w: &mut World, at: Vec3) {
    let p = [at.x, at.y, at.z];
    let mut ev = w.resource_mut::<Events>();
    ev.host.sound("entity.shulker_bullet.hit", p, 1.0, 1.0);
    ev.host.particles("minecraft:explosion", p, [0.2; 3], 0.0, 2);
}

/// A player's blow on a flying bullet (`hit_shulker_bullet`): the hurt sound and fifteen crit particles, then the bullet is gone. No
/// game event and no hurt; a stuck bullet is not a target. True when the bullet was destroyed.
pub fn struck(w: &mut World, e: Entity) -> bool {
    if w.get::<Homing>(e).is_none() || w.get::<Stuck>(e).is_some() {
        return false;
    }
    let Some(at) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    let p = [at.x, at.y, at.z];
    {
        let mut ev = w.resource_mut::<Events>();
        ev.host.sound("entity.shulker_bullet.hurt", p, 1.0, 1.0);
        ev.host.particles("minecraft:crit", p, [0.2; 3], 0.0, 15);
    }
    w.despawn(e);
    true
}
