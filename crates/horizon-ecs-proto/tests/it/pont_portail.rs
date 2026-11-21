//! The portals through the bridge (lot 73): a forced death, the entries of the portals that moved, the rule of `NoPortal`, and the size event. Written, not run.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_client_lib::mobs::world::leaked_test_world_mut;
use horizon_core::geometry::Vec3;
use horizon_core::nbt::{JavaString, Tag};
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup};
use horizon_ecs_proto::cow::{Body, Moving, Pos};
use horizon_ecs_proto::portal::NoPortal;
use horizon_ecs_proto::seat::{self, SeatKind, Seats};

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

#[test]
fn kill_removes_a_mob() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    h.kill(mw, e);
    assert!(h.world().get_entity(e).is_err(), "the cow is gone");
}

#[test]
fn kill_under_veto_stays_alive() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    h.set_death_vetoable(true);
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    h.kill(mw, e);
    assert!(h.world().get_entity(e).is_ok(), "the veto holds the cow");
    assert!(h.world().get::<horizon_ecs_proto::cow::Dying>(e).is_some(), "the cow waits for the host's answer");
}

#[test]
fn portal_entries_visit_only_the_moved() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let still = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    let moved = h.spawn(mw, "minecraft:cow", [2.5, 1.0, 0.5], None).expect("a known species");
    h.world_mut().entity_mut(still).remove::<Moving>();
    h.world_mut().entity_mut(moved).insert(Moving);
    h.world_mut().get_mut::<Pos>(moved).unwrap().0 = Vec3::new(4.5, 1.0, 0.5);
    let rows = h.portal_entries(mw);
    assert!(rows.iter().all(|r| r.0 != still), "an entity at rest is not visited");
    let row = rows.iter().find(|r| r.0 == moved).expect("the moved entity is visited");
    assert_eq!(row.2, [4.5, 1.0, 0.5], "it is seen where it is now");
}

#[test]
fn no_portal_entity_cannot_use_a_portal() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let cow = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(h.can_use_portal(cow), "a cow takes a portal");
    h.world_mut().entity_mut(cow).insert(NoPortal);
    assert!(!h.can_use_portal(cow), "a NoPortal entity takes none");
}

#[test]
fn a_boat_keeps_its_two_passengers_through_a_save() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let boat = horizon_ecs_proto::boat_state::spawn(h.world_mut(), 0, false, [0.5, 64.0, 0.5], 0.0);
    h.world_mut().entity_mut(boat).insert(Seats::new(SeatKind::Boat { chest: false, raft: false }));
    let a = h.spawn(mw, "minecraft:cow", [0.5, 64.0, 0.5], None).expect("a known species");
    let b = h.spawn(mw, "minecraft:cow", [0.5, 64.0, 0.5], None).expect("a known species");
    assert!(h.board(mw, a, boat, None) && h.board(mw, b, boat, None), "two creatures take seats");
    let nbt = h.entity_nbt(boat).expect("a saved boat");
    assert!(matches!(nbt.get(&JavaString::from("Passengers")), Some(Tag::List(l)) if l.len() == 2), "the boat writes its two riders");
    let back = h.load_entity(mw, &nbt).expect("the boat is restored");
    assert_eq!(seat::riders_of(h.world(), back).len(), 2, "both riders are seated again");
}

#[test]
fn a_boat_gets_its_seats_at_boarding_and_loses_them_when_empty() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let boat = horizon_ecs_proto::boat_state::spawn(h.world_mut(), 0, false, [0.5, 64.0, 0.5], 0.0);
    assert!(h.world().get::<Seats>(boat).is_none(), "a new boat has no seats");
    let cow = h.spawn(mw, "minecraft:cow", [0.5, 64.0, 0.5], None).expect("a known species");
    assert!(h.board(mw, cow, boat, None), "the cow takes a seat");
    assert!(h.world().get::<Seats>(boat).is_some(), "the boarding puts the seats on the boat");
    assert!(h.leave(mw, cow).is_some(), "the cow gets off");
    assert!(h.world().get::<Seats>(boat).is_none(), "an empty boat has no seats");
}
