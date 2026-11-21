//! Lot E5, the fishing hook and the shulker bullet against the original: a hooked creature is followed at 0.8 of its height, a blow
//! destroys a flying bullet (its sound and particles, no hurt), and a bullet pursues its creature and hurts it on impact.
//! `HORIZON_TEST_RESOURCES` set. The hook's deflection by `deflects_projectiles` is not tested here: it needs a deflecting species.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::behaviours::homing_projectile;
use horizon_ecs_proto::cow::{species, Herd, Pos};
use horizon_ecs_proto::fishing_hook::{cast, Hook, Phase, Target};
use horizon_ecs_proto::goals::{Events, Online, Player};
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::shulker_bullet;
use horizon_ecs_proto::species::shulker;
use horizon_ecs_proto::targeting::Who;

/// The player who casts, standing at `at`.
fn stand(h: &mut Herd, at: Vec3) {
    h.world.resource_mut::<Online>().set(Player { id: 1, x: at.x, y: at.y, z: at.z, main: 0, off: 0, ..Default::default() });
}
fn drain(h: &mut Herd) -> Vec<HostEvent> {
    h.world.resource_mut::<Events>().host.drain().collect()
}

/// A hooked creature: the hook sits at its feet plus 0.8 of its height, tick by tick, while the creature stays.
#[test]
fn a_hook_holding_a_creature_sits_at_eight_tenths_of_its_height() {
    if !have_data() {
        return;
    }
    let owner = Vec3::new(0.5, 1.5, 0.5);
    let mut h = Herd::with_slab(16, true, false);
    let cow = ecs_cow(&mut h, 3.5_f64.max(1.5 + f64::from(species().width)), 1.0, 0.5, 7);
    h.tick();
    let at_rest = pin(&h, cow);
    stand(&mut h, owner);
    let hook = cast(&mut h.world, 1, owner, Vec3::new(1.0, 0.0, 0.0), 0, 0, 7);
    let mut hooked = false;
    for _ in 1..=20 {
        stand(&mut h, owner);
        tick_pinned(&mut h, cow, &at_rest);
        if h.world.get::<Hook>(hook).map(|x| x.phase) == Some(Phase::Hooked(Target::Entity(cow))) {
            hooked = true;
            break;
        }
    }
    assert!(hooked, "the hook holds the cow within 20 ticks");
    let height = f64::from(species().height);
    for t in 1..=3 {
        stand(&mut h, owner);
        tick_pinned(&mut h, cow, &at_rest);
        let feet = h.world.get::<Pos>(cow).unwrap().0;
        let got = h.world.get::<Pos>(hook).unwrap().0;
        let want_y = feet.y + height * 0.8;
        assert!((got.y - want_y).abs() < 1e-9 && got.x == feet.x && got.z == feet.z, "tick {t}: hook {got:?}, want y {want_y}");
    }
}

/// A bullet of a shulker, from `owner` at `target`: the flight of the homing projectile, launched by its own species' numbers.
fn shoot(h: &mut Herd, owner: Entity, target: Entity) -> Entity {
    homing_projectile::spawn(&mut h.world, owner, Who::Entity(target), 1, 0x5eed, &shulker::SHOT);
    h.world.query_filtered::<Entity, With<homing_projectile::Homing>>().iter(&h.world).next().expect("a homing bullet")
}

/// A player's blow on a flying bullet: the hurt sound and fifteen crit particles, the bullet gone, no hurt and no game event.
#[test]
fn a_blow_destroys_a_flying_bullet_without_hurting_its_target() {
    if !have_data() {
        return;
    }
    let (_, mut h) = flat_pair();
    let owner = ecs_cow(&mut h, 0.5, 1.0, 0.5, 5);
    let target = ecs_cow(&mut h, 4.5, 1.0, 0.5, 6);
    h.world.entity_mut(target).insert(horizon_ecs_proto::cow::Health(1000.0));
    h.tick();
    let at = pin(&h, target);
    let before = health(&h, target);
    let bullet = shoot(&mut h, owner, target);
    for _ in 0..3 {
        tick_pinned(&mut h, target, &at);
    }
    drain(&mut h);
    assert!(shulker_bullet::struck(&mut h.world, bullet), "the blow destroys the bullet");
    let out = drain(&mut h);
    assert!(h.world.get_entity(bullet).is_err(), "the bullet is gone");
    assert!(out.iter().any(|x| matches!(x, HostEvent::Sound { name: "entity.shulker_bullet.hurt", .. })), "{out:?}");
    assert!(out.iter().any(|x| matches!(x, HostEvent::Particles { kind: "minecraft:crit", count: 15, .. })), "{out:?}");
    assert!(!out.iter().any(|x| matches!(x, HostEvent::GameEvent { .. })), "no game event at a blow: {out:?}");
    tick_pinned(&mut h, target, &at);
    assert_eq!(before, health(&h, target), "the target is not hurt by a blow on the bullet");
}

/// A bullet pursues its creature (closer after three ticks than at the launch), then hits it: the creature is hurt and the bullet is gone.
#[test]
fn a_bullet_pursues_its_creature_and_hurts_it_on_impact() {
    if !have_data() {
        return;
    }
    let (_, mut h) = flat_pair();
    let owner = ecs_cow(&mut h, 0.5, 1.0, 0.5, 5);
    let target = ecs_cow(&mut h, 4.5, 1.0, 0.5, 6);
    h.world.entity_mut(target).insert(horizon_ecs_proto::cow::Health(1000.0));
    h.tick();
    let at = pin(&h, target);
    let before = health(&h, target);
    let bullet = shoot(&mut h, owner, target);
    let gap = |h: &Herd| {
        let (b, c) = (h.world.get::<Pos>(bullet).map(|p| p.0), h.world.get::<Pos>(target).map(|p| p.0));
        match (b, c) {
            (Some(b), Some(c)) => b.distance(c),
            _ => f64::INFINITY,
        }
    };
    let start = gap(&h);
    let mut out = Vec::new();
    for _ in 0..3 {
        tick_pinned(&mut h, target, &at);
    }
    assert!(gap(&h) < start, "the bullet closes on its creature");
    for _ in 0..200 {
        tick_pinned(&mut h, target, &at);
        out.extend(h.drain_host_out());
        if h.world.get_entity(bullet).is_err() {
            break;
        }
    }
    assert!(h.world.get_entity(bullet).is_err(), "the bullet is gone at the creature");
    tick_pinned(&mut h, target, &at);
    out.extend(h.drain_host_out());
    assert!(before > health(&h, target), "the creature is hurt");
    assert!(out.iter().any(|x| matches!(x, HostEvent::Sound { name: "entity.shulker_bullet.hurt", .. })), "{out:?}");
}
