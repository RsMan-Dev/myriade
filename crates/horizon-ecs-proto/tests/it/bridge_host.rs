//! The host world of the bridge (greffe L0, lot 01): the world builds over a level with no blocks of its own; spawn and ids, the players'
//! facts, the chunks' NBT, the queries and the block changes it reports, which are all that the host sees of it.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup, PlayerFact, Select};
use horizon_client_lib::mobs::world::leaked_test_world_mut;
use horizon_ecs_proto::cow::{BlockChanges, Pos};
use horizon_ecs_proto::goals::Player;
use horizon_ecs_proto::host_out::HostEvent;

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

fn fact() -> PlayerFact {
    PlayerFact { alive: true, targetable: true, hero: None, killer: Default::default() }
}

fn player(id: u32, x: f64, z: f64) -> Player {
    Player { id, x, y: 1.0, z, ..Default::default() }
}

#[test]
fn spawn_then_id_reaches_the_same_entity() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    let mut next = 40;
    let id = h.id_for(e, &mut || {
        next += 1;
        next
    });
    assert_eq!(id, 41, "the first free id is given");
    assert_eq!(h.entity_of(id), Some(e));
    assert_eq!(h.id_for(e, &mut || 99), id, "an entity keeps its id");
}

#[test]
fn set_player_then_remove_leaves_no_fact() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    h.set_player(player(3, 1.0, 1.0), fact());
    assert!(h.world().resource::<horizon_ecs_proto::bridge::players::PlayerFacts>().0.contains_key(&3));
    h.remove_player(3);
    assert!(h.world().resource::<horizon_ecs_proto::bridge::players::PlayerFacts>().0.is_empty());
    assert!(h.world().resource::<horizon_ecs_proto::goals::Online>().get(3).is_none());
}

#[test]
fn no_player_means_nothing_wakes() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    let at = h.world().get::<Pos>(e).unwrap().0;
    assert!(h.step(mw).is_empty());
    assert_eq!(h.world().get::<Pos>(e).unwrap().0, at, "at 0 players the schedule does not run");
}

#[test]
fn block_change_is_reported_as_set_block() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    h.set_player(player(1, 0.5, 0.5), fact());
    h.world_mut().resource_mut::<BlockChanges>().0.push((1, 0, 1, "minecraft:stone"));
    let out = h.step(mw);
    assert!(out.iter().any(|ev| matches!(ev, HostEvent::SetBlock { pos: [1, 0, 1], .. })), "the change goes to the host");
}

#[test]
fn select_by_kind_and_radius_finds_the_cows_near() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let near = h.spawn(mw, "minecraft:cow", [2.5, 1.0, 2.5], None).expect("a known species");
    let far = h.spawn(mw, "minecraft:cow", [40.5, 1.0, 40.5], None).expect("a known species");
    let q = Select { kind: Some("minecraft:cow".into()), near: Some(([0.5, 1.0, 0.5], 8.0)), ..Default::default() };
    let found = h.select(&q);
    assert_eq!(found, vec![near]);
    assert!(!found.contains(&far));
}

#[test]
fn save_chunk_then_load_gives_the_same_nbt() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    let saved = h.save_chunk(0, 0);
    assert_eq!(saved.len(), 1);
    let back = h.load_entity(mw, &saved[0]).expect("the entity is made again");
    assert_eq!(h.entity_nbt(back), h.entity_nbt(e));
}
