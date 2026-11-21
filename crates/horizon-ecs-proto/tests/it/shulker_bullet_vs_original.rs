//! The shulker bullet's ends against the original's `ShulkerBullet`: the sound and particles of a block or a creature hit, and gone with them.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{cow::*, host_out::HostEvent, projectile::*};

/// A bullet thrown into a block: the hit sound and two explosion particles at its end, and it is gone with them.
#[test]
fn a_bullet_that_hits_a_block_plays_its_sound_and_is_gone() {
    if !have_data() {
        return;
    }
    let (mut r, mut h) = flat_pair();
    r.set_block(0, 1, 2, "minecraft:stone");
    r.apply_block_events();
    let flight = Flight { bullet: true, damage: Damage::Fixed(4.0), ..Flight::SNOWBALL };
    let e = launch(&mut h.world, flight, Vec3::new(0.5, 1.5, 0.5), Vec3::new(0.0, 0.0, 0.4), None);
    let mut out = Vec::new();
    for _ in 0..40 {
        h.tick();
        out.extend(h.drain_host_out());
        if h.world.get_entity(e).is_err() {
            break;
        }
    }
    assert!(h.world.get_entity(e).is_err(), "the bullet is gone at the block");
    assert!(out.iter().any(|x| matches!(x, HostEvent::Sound { name: "entity.shulker_bullet.hit", .. })), "{out:?}");
    assert!(out.iter().any(|x| matches!(x, HostEvent::Particles { kind: "minecraft:explosion", count: 2, .. })), "{out:?}");
}

/// A bullet that hits a creature: the hurt sound, the game event at the blow, the creature hurt, and the bullet gone.
#[test]
fn a_bullet_that_hits_a_creature_plays_its_hurt_and_is_gone() {
    if !have_data() {
        return;
    }
    let (_, mut h) = flat_pair();
    let cow = ecs_cow(&mut h, 0.5, 1.0, 2.5, 5);
    h.tick();
    let before = health(&h, cow);
    let flight = Flight { bullet: true, damage: Damage::Fixed(4.0), ..Flight::SNOWBALL };
    let e = launch(&mut h.world, flight, Vec3::new(0.5, 1.5, 0.5), Vec3::new(0.0, 0.0, 0.4), None);
    let mut out = Vec::new();
    for _ in 0..40 {
        h.tick();
        out.extend(h.drain_host_out());
        if h.world.get_entity(e).is_err() {
            break;
        }
    }
    assert!(h.world.get_entity(e).is_err(), "the bullet is gone at the creature");
    assert!(before > health(&h, cow), "the creature is hurt");
    assert!(out.iter().any(|x| matches!(x, HostEvent::Sound { name: "entity.shulker_bullet.hurt", .. })), "{out:?}");
    assert!(out.iter().any(|x| matches!(x, HostEvent::GameEvent { name: "minecraft:entity_damage", .. })), "{out:?}");
}
