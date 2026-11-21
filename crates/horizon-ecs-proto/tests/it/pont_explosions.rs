//! The host's blasts and splashes over the bridge (lot 47): a blast hurts and leaves the blocks unless asked, the source of a blast is
//! spared, water puts a fire out, and an indirect magic hit names its owner as the last attacker.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_client_lib::mobs::{MobTypes, MobWorld, world::leaked_test_world_mut};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup, HurtSource, PlayerFact};
use horizon_ecs_proto::cow::{Burning, Health, LastHurtBy, set_on_fire};
use horizon_ecs_proto::goals::Player;
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::projectile::Blast;
use horizon_ecs_proto::targeting::Who;
use std::sync::Arc;

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

fn fact() -> PlayerFact {
    PlayerFact { alive: true, targetable: true, ..Default::default() }
}

/// A player far enough from the blasts (10 blocks, beyond their diameter) to stay out of them, and near enough to keep the creatures awake.
fn player() -> Player {
    Player { id: 1, x: 10.5, y: 1.0, z: 0.5, ..Default::default() }
}

fn blast(at: Vec3, source: Option<Entity>) -> Blast {
    Blast { at, radius: 3.0, fire: false, owner: None, source }
}

/// A world with a stone floor at y -2..=0 under the spawn point, and no block over it.
fn stone_world() -> (&'static MobWorld<'static>, &'static MobTypes) {
    let (mw, types, _) = leaked_test_world_mut(true, |blocks, registry| {
        let stone = registry.default_state("minecraft:stone").unwrap();
        for cx in -4..4 {
            for cz in -4..4 {
                blocks.chunks.insert((cx, cz), Arc::new(horizon_client_render::chunk::Chunk::empty(cx, cz)));
            }
        }
        for x in -20..20 {
            for z in -20..20 {
                for y in -2..=0 {
                    blocks.set(x, y, z, stone).unwrap();
                }
            }
        }
    })
    .expect("HORIZON_TEST_RESOURCES");
    (mw, types)
}

fn tags() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"))
}

#[test]
fn explode_with_false_hurts_the_creatures_and_changes_no_block() {
    have_data();
    let (mw, types) = stone_world();
    let mut h = EcsWorld::new(&setup(), mw, types, &tags());
    h.set_player(player(), fact());
    let near = h.spawn(mw, "minecraft:cow", [1.5, 1.0, 0.5], None).expect("a known species");
    let before = h.world().get::<Health>(near).unwrap().0;
    h.explode_with(mw, blast(Vec3::new(0.5, 1.0, 0.5), None), false);
    let out = h.step(mw);
    assert!(!out.iter().any(|ev| matches!(ev, HostEvent::SetBlock { .. })), "the blocks stay");
    assert!(h.world().get::<Health>(near).unwrap().0 < before, "the creature in reach is hurt");
}

#[test]
fn explode_with_true_breaks_the_blocks_of_the_floor() {
    have_data();
    let (mw, types) = stone_world();
    let mut h = EcsWorld::new(&setup(), mw, types, &tags());
    h.set_player(player(), fact());
    h.explode_with(mw, blast(Vec3::new(0.5, 1.0, 0.5), None), true);
    let out = h.step(mw);
    assert!(out.iter().any(|ev| matches!(ev, HostEvent::SetBlock { .. })), "the stone under the blast breaks");
}

#[test]
fn the_source_of_a_blast_is_not_hurt_by_it() {
    have_data();
    let (mw, types) = stone_world();
    let mut h = EcsWorld::new(&setup(), mw, types, &tags());
    h.set_player(player(), fact());
    let source = h.spawn(mw, "minecraft:cow", [1.5, 1.0, 0.5], None).expect("a known species");
    let other = h.spawn(mw, "minecraft:cow", [2.5, 1.0, 0.5], None).expect("a known species");
    let (hs, ho) = (h.world().get::<Health>(source).unwrap().0, h.world().get::<Health>(other).unwrap().0);
    h.explode(mw, blast(Vec3::new(1.5, 1.0, 0.5), Some(source)));
    h.step(mw);
    assert_eq!(h.world().get::<Health>(source).unwrap().0, hs, "the source is spared");
    assert!(h.world().get::<Health>(other).unwrap().0 < ho, "the others are hurt");
}

#[test]
fn splashed_puts_out_the_fire_of_a_creature() {
    have_data();
    let (mw, types) = stone_world();
    let mut h = EcsWorld::new(&setup(), mw, types, &tags());
    let e = h.spawn(mw, "minecraft:cow", [1.5, 1.0, 0.5], None).expect("a known species");
    set_on_fire(h.world_mut(), e, 100);
    assert!(h.world().get::<Burning>(e).is_some(), "the cow burns");
    h.splashed(mw, e);
    assert!(h.world().get::<Burning>(e).is_none(), "the water puts the fire out");
}

#[test]
fn indirect_magic_names_its_owner_as_the_last_attacker() {
    have_data();
    let (mw, types) = stone_world();
    let mut h = EcsWorld::new(&setup(), mw, types, &tags());
    h.set_player(player(), fact());
    let target = h.spawn(mw, "minecraft:cow", [1.5, 1.0, 0.5], None).expect("a known species");
    let owner = h.spawn(mw, "minecraft:cow", [3.5, 1.0, 0.5], None).expect("a known species");
    h.hurt(mw, target, 1.0, HurtSource::IndirectMagic { owner: Some(Who::Entity(owner)) });
    h.step(mw);
    let last = h.world().get::<LastHurtBy>(target).expect("the target was hurt");
    assert_eq!(last.who, Who::Entity(owner), "the owner is the last attacker");
}
