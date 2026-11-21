//! The reads of an entity's body and identity by `Entity` (lot 40): each reads its component, and a sleeping entity reads as absent.
use crate::common::*;
use bevy_ecs::{entity_disabling::Disabled, prelude::*};
use horizon_client_lib::mobs::world::leaked_test_world_mut;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup, HurtSource, PlayerFact, Select};
use horizon_ecs_proto::bridge::select;
use horizon_ecs_proto::cow::PushFree;
use horizon_ecs_proto::goals::{self, Player};
use horizon_ecs_proto::targeting::Eye;

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

fn cow_world_tags() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"))
}

#[test]
fn bounds_move_with_a_teleport() {
    have_data();
    let tags = cow_world_tags();
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    let before = h.bounds(e).expect("a cow has a box").min_position().x;
    h.teleport(mw, e, [10.5, 1.0, 0.5]);
    let after = h.bounds(e).expect("the box stays").min_position().x;
    assert!((after - before - 10.0).abs() < 1e-9, "the box moves by the teleport: {before} -> {after}");
}

#[test]
fn alive_ends_with_a_lethal_hit() {
    have_data();
    let tags = cow_world_tags();
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    h.set_player(Player { id: 1, x: 0.5, y: 1.0, z: 0.5, ..Default::default() }, PlayerFact { alive: true, targetable: true, hero: None, killer: Default::default() });
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(h.alive(e), "a fresh cow is alive");
    h.hurt(mw, e, 1e6, HurtSource::Generic);
    h.step(mw);
    assert!(!h.alive(e), "a lethal hit and a step: not alive");
}

#[test]
fn motion_follows_set_velocity() {
    have_data();
    let tags = cow_world_tags();
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    goals::set_velocity(h.world_mut(), e, Vec3::new(0.25, 0.0, -0.5));
    let m = h.motion(e).expect("a cow moves");
    assert_eq!(m.velocity, [0.25, 0.0, -0.5]);
    assert_eq!(h.velocity(e).map(|v| [v.x, v.y, v.z]), Some([0.25, 0.0, -0.5]));
}

#[test]
fn pushable_is_false_for_an_object() {
    have_data();
    let tags = cow_world_tags();
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(h.pushable(e), "a living cow is pushed");
    h.world_mut().entity_mut(e).insert(PushFree);
    assert!(!h.pushable(e), "an object on the ground is not pushed");
}

#[test]
fn eye_offset_is_none_without_eye() {
    have_data();
    let tags = cow_world_tags();
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert_eq!(h.eye_offset(e), None, "no targeting, no eye");
    h.world_mut().entity_mut(e).insert(Eye(0.5));
    assert_eq!(h.eye_offset(e), Some(0.5));
}

#[test]
fn species_count_follows_spawn_and_despawn() {
    have_data();
    let tags = cow_world_tags();
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let before = h.species_count("minecraft:cow");
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert_eq!(h.species_count("minecraft:cow"), before + 1, "the spawn is counted");
    h.despawn(e);
    assert_eq!(h.species_count("minecraft:cow"), before, "the despawn is not");
    assert_eq!(h.species_count("minecraft:no_such_mob"), 0, "an unknown species counts 0");
}

#[test]
fn count_with_a_sphere_is_the_length_of_select() {
    have_data();
    let tags = cow_world_tags();
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    for x in [0.5, 1.5, 2.5] {
        h.spawn(mw, "minecraft:cow", [x, 1.0, 0.5], None).expect("a known species");
    }
    let q = Select { kind: Some("minecraft:cow".to_string()), tag: None, near: Some(([0.5, 1.0, 0.5], 8.0)), limit: 0 };
    assert_eq!(select::count(h.world(), &q), 3, "three cows within the sphere");
    assert_eq!(select::count(h.world(), &q), h.select(&q).len(), "count is the length of select");
}

#[test]
fn a_sleeping_entity_reads_as_absent() {
    have_data();
    let tags = cow_world_tags();
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    h.world_mut().entity_mut(e).insert(Disabled);
    assert_eq!(h.bounds(e), None);
    assert_eq!(h.motion(e), None);
    assert_eq!(h.velocity(e), None);
    assert_eq!(h.yaw(e), None);
    assert_eq!(h.health(e), None);
    assert_eq!(h.species(e), None);
    assert!(!h.alive(e));
    assert!(!h.pushable(e));
}
