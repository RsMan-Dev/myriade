//! The lot-46 outputs of the host: each one is written at its fact and read by the host's next `step`; nothing comes out of a world at rest.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_client_lib::mobs::world::leaked_test_world_mut;
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup, HurtSource, PlayerFact};
use horizon_ecs_proto::cow::{Dying, Health};
use horizon_ecs_proto::goals::Player;
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::targeting::{Target, Who};
use horizon_ecs_proto::{leash::{self, Holder}, tame};

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

fn fact() -> PlayerFact {
    PlayerFact { alive: true, targetable: true }
}

fn player(id: u32, x: f64, z: f64) -> Player {
    Player { id, x, y: 1.0, z, ..Default::default() }
}

fn tags() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"))
}

#[test]
fn lethal_hit_without_veto_removes_the_entity_and_says_death() {
    have_data();
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags());
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    h.set_player(player(1, 0.5, 0.5), fact());
    h.hurt(mw, e, 1000.0, HurtSource::Generic);
    let out = h.step(mw);
    assert!(out.iter().any(|ev| matches!(ev, HostEvent::Death { entity, .. } if *entity == e)));
    assert!(h.world().get_entity(e).is_err(), "the default policy removes the entity");
}

#[test]
fn veto_keeps_the_entity_dying_until_it_is_resolved() {
    have_data();
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags());
    h.set_death_vetoable(true);
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    h.set_player(player(1, 0.5, 0.5), fact());
    h.hurt(mw, e, 1000.0, HurtSource::Generic);
    let out = h.step(mw);
    assert!(out.iter().any(|ev| matches!(ev, HostEvent::Death { entity, .. } if *entity == e)));
    assert!(h.world().get::<Dying>(e).is_some(), "the lethal hit is held");
    h.resolve_death(e, Some(2.0));
    assert_eq!(h.world().get::<Health>(e).map(|x| x.0), Some(2.0));
    assert!(h.world().get::<Dying>(e).is_none());
    assert!(h.step(mw).iter().all(|ev| !matches!(ev, HostEvent::Death { .. })), "a creature that lives on is not said dead again");
}

#[test]
fn resolve_death_keeps_a_player_target() {
    have_data();
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags());
    h.set_death_vetoable(true);
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    h.set_player(player(1, 0.5, 0.5), fact());
    h.world_mut().entity_mut(e).insert(Target::new(Who::Player(1), 20, 0));
    h.hurt(mw, e, 1000.0, HurtSource::Generic);
    h.step(mw);
    h.resolve_death(e, Some(2.0));
    assert_eq!(h.world().get::<Health>(e).map(|x| x.0), Some(2.0));
    assert_eq!(h.world().get::<Target>(e).map(|t| t.who), Some(Who::Player(1)), "the revived entity keeps its target");
    assert!(h.step(mw).iter().all(|ev| !matches!(ev, HostEvent::Death { .. })), "a creature that lives on is not said dead again");
}

#[test]
fn a_removed_id_comes_out_once() {
    have_data();
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags());
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    let id = h.id_for(e, &mut || 77);
    assert!(h.drain_removed().is_empty());
    h.world_mut().despawn(e);
    assert_eq!(h.drain_removed(), vec![id]);
    assert!(h.drain_removed().is_empty());
}

#[test]
fn leash_detach_is_said() {
    have_data();
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags());
    let a = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    let b = h.spawn(mw, "minecraft:cow", [1.5, 1.0, 0.5], None).expect("a known species");
    h.set_player(player(1, 0.5, 0.5), fact());
    assert!(leash::attach(h.world_mut(), a, Holder::Entity(b)), "the lead did not go on");
    assert!(leash::detach(h.world_mut(), a).is_some());
    let out = h.step(mw);
    assert!(out.iter().any(|ev| matches!(ev, HostEvent::Unleashed { entity } if *entity == a)));
}

#[test]
fn taming_by_a_player_is_said() {
    have_data();
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags());
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    h.set_player(player(1, 0.5, 0.5), fact());
    tame::tame(h.world_mut(), e, Who::Player(4), 20.0);
    let out = h.step(mw);
    assert!(out.iter().any(|ev| matches!(ev, HostEvent::Tamed { entity, owner: Who::Player(4) } if *entity == e)));
}

#[test]
fn an_apparition_is_said() {
    have_data();
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags());
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    let out = h.step(mw);
    assert!(out.iter().any(|ev| matches!(ev, HostEvent::Spawned { entity, natural: false } if *entity == e)));
}

#[test]
fn a_world_at_rest_says_nothing() {
    have_data();
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags());
    assert!(h.step(mw).is_empty());
}

#[test]
fn a_dying_entity_is_looted_before_the_host_removes_it() {
    have_data();
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags());
    h.set_death_vetoable(true);
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    h.set_player(player(1, 0.5, 0.5), fact());
    h.hurt(mw, e, 1000.0, HurtSource::Generic);
    h.step(mw);
    assert!(h.loot_dying(e), "the loot goes first");
    assert!(h.world().get::<Dying>(e).is_none() && h.world().get_entity(e).is_ok(), "the entity stays for the hook");
    assert!(!h.loot_dying(e), "the loot leaves once");
    h.despawn(e);
    assert!(h.world().get_entity(e).is_err());
}

#[test]
fn a_totem_saves_from_a_lethal_hit_unless_the_host_refuses() {
    use horizon_client_lib::items::ItemStack;
    have_data();
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags());
    h.set_player(player(1, 0.5, 0.5), fact());
    let a = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    h.set_equipment_stack(a, 1, ItemStack::new("minecraft:totem_of_undying", 1));
    h.hurt(mw, a, 1000.0, HurtSource::Generic);
    let out = h.step(mw);
    assert_eq!(h.world().get::<Health>(a).map(|x| x.0), Some(1.0), "unwatched: the totem is used at once");
    assert!(!out.iter().any(|ev| matches!(ev, HostEvent::TotemAsk { .. } | HostEvent::Death { .. })));
    h.set_totem_watched(true);
    let b = h.spawn(mw, "minecraft:cow", [1.5, 1.0, 0.5], None).expect("a known species");
    h.set_equipment_stack(b, 0, ItemStack::new("minecraft:totem_of_undying", 1));
    h.hurt(mw, b, 1000.0, HurtSource::Generic);
    let out = h.step(mw);
    assert!(out.iter().any(|ev| matches!(ev, HostEvent::TotemAsk { entity, off_hand: false, .. } if *entity == b)));
    assert!(h.equipment_stack(b, 0).is_some(), "the totem waits for the answer");
    assert!(!h.resolve_totem(b, true), "refused: it dies");
    assert!(h.step(mw).iter().any(|ev| matches!(ev, HostEvent::Death { entity, .. } if *entity == b)));
}
