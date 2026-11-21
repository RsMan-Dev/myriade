//! The vehicles and the trade of the bridge (greffe lot 45): a rider on a mount, the seats, the boarding time, the mount's saddle and the
//! offers of a merchant, read and written by entity through the host world.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;
use horizon_client_lib::mobs::world::leaked_test_world_mut;
use horizon_ecs_proto::behaviours::trade_offers::Merchant;
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup, PlayerFact};
use horizon_ecs_proto::cow::now_of;
use horizon_ecs_proto::goals::Player;
use horizon_ecs_proto::mount::{self, Mount};
use horizon_ecs_proto::targeting::Who;

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

fn fact() -> PlayerFact {
    PlayerFact { alive: true, targetable: true }
}

fn player(id: u32, x: f64, z: f64) -> Player {
    Player { id, x, y: 1.0, z, ..Default::default() }
}

#[test]
fn ride_then_host_vehicle_and_riders_find_each_other() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let horse = h.spawn(mw, "minecraft:horse", [0.5, 1.0, 0.5], None).expect("a known species");
    let cow = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(mount::ride(h.world_mut(), horse, Who::Entity(cow)));
    assert_eq!(h.host_vehicle(cow), Some(horse));
    assert_eq!(h.host_riders(horse), vec![cow]);
    assert!(h.is_passenger(cow));
    assert!(!h.is_passenger(horse));
}

#[test]
fn take_passengers_moves_the_rider_to_the_other_mount() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let from = h.spawn(mw, "minecraft:horse", [0.5, 1.0, 0.5], None).expect("a known species");
    let to = h.spawn(mw, "minecraft:horse", [4.5, 1.0, 0.5], None).expect("a known species");
    let cow = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(mount::ride(h.world_mut(), from, Who::Entity(cow)));
    h.take_passengers(from, to);
    assert_eq!(h.host_vehicle(cow), Some(to));
    assert!(h.host_riders(from).is_empty());
    assert_eq!(h.host_riders(to), vec![cow]);
}

#[test]
fn a_full_mount_has_no_free_seat() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let horse = h.spawn(mw, "minecraft:horse", [0.5, 1.0, 0.5], None).expect("a known species");
    let cow = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(h.has_free_seat(horse));
    assert!(mount::ride(h.world_mut(), horse, Who::Entity(cow)));
    assert!(!h.has_free_seat(horse), "one seat, taken");
}

#[test]
fn boarding_waits_for_ready_at_then_is_ready() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let horse = h.spawn(mw, "minecraft:horse", [0.5, 1.0, 0.5], None).expect("a known species");
    h.set_player(player(1, 0.5, 0.5), fact());
    let now = now_of(h.world(), horse);
    h.world_mut().get_mut::<Mount>(horse).expect("a mount").ready_at = now + 5;
    assert!(!h.boarding_ready(horse), "before ready_at");
    assert!(h.boarding_cooldown(horse).is_some());
    for _ in 0..200 {
        if h.boarding_ready(horse) {
            break;
        }
        h.step(mw);
    }
    assert!(h.boarding_ready(horse), "after the schedule ran past ready_at");
    assert!(h.boarding_cooldown(horse).is_none());
}

#[test]
fn stored_saddle_is_read_back_by_mount_contents() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let horse = h.spawn(mw, "minecraft:horse", [0.5, 1.0, 0.5], None).expect("a known species");
    h.mount_store(horse, Some(ItemStack::new("minecraft:saddle", 1)), None, Vec::new());
    let contents = h.mount_contents(horse).expect("a mount has contents");
    assert_eq!(contents.saddle.as_ref().map(|s| &*s.item), Some("minecraft:saddle"));
    assert!(h.world().get::<Mount>(horse).is_some_and(|m| m.saddled), "a saddle saddles the mount");
}

#[test]
fn offers_set_by_the_host_keep_the_trader_coherent() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let trader = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    h.world_mut().entity_mut(trader).insert(Merchant::new(true, 0, 0));
    assert_eq!(h.trader_opaque(trader), Some(false), "a wandering trader is never opaque");
    h.set_trade_offers(trader, Vec::new());
    assert_eq!(h.trade_offers(trader).map(|o| o.len()), Some(0), "the offers set are the offers read");
    assert_eq!(h.trader_opaque(trader), Some(false));
    assert!(!h.trade_busy(trader), "no screen is open");
}
