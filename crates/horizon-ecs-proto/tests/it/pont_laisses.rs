//! The leads through the bridge: the holder saved as an entity's uuid or a knot's coordinates, a read lead tied once its holder appears,
//! the snapping reach, the uuid index, and the creature and vehicle markers of a view.
use crate::common::*;
use horizon_client_lib::mobs::world::leaked_test_world_mut;
use horizon_core::{geometry::Vec3, nbt::Tag};
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup};
use horizon_ecs_proto::bridge::view::view;
use horizon_ecs_proto::boat::Boat;
use horizon_ecs_proto::cow::{Pos, Vel};
use horizon_ecs_proto::leash::{self, Holder};

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

#[test]
fn attach_to_a_knot_then_save_writes_its_coordinates() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let cow = h.spawn(mw, "minecraft:cow", [3.5, 64.0, 5.5], None).expect("a cow");
    assert!(leash::attach(h.world_mut(), cow, Holder::Knot([3, 64, 5])), "the lead goes on");
    let mut tag = Tag::Compound(Default::default());
    h.leash_save(cow, &mut tag);
    assert_eq!(tag.get("leash"), Some(&Tag::IntArray(vec![3, 64, 5])));
}

#[test]
fn attach_to_an_entity_saves_its_uuid() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let holder = h.spawn(mw, "minecraft:cow", [0.5, 64.0, 0.5], None).expect("a cow");
    let cow = h.spawn(mw, "minecraft:cow", [1.5, 64.0, 0.5], None).expect("a cow");
    let uuid = [1, -2, 3, -4];
    h.set_uuid(holder, uuid);
    assert!(leash::attach(h.world_mut(), cow, Holder::Entity(holder)), "a creature may hold another");
    let mut tag = Tag::Compound(Default::default());
    h.leash_save(cow, &mut tag);
    assert_eq!(tag.get("leash").and_then(|l| l.get("UUID")), Some(&Tag::IntArray(uuid.to_vec())));
}

#[test]
fn a_pending_lead_waits_for_its_uuid_then_ties_when_the_entity_appears() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let cow = h.spawn(mw, "minecraft:cow", [0.5, 64.0, 0.5], None).expect("a cow");
    let uuid = [9, 9, 9, 9];
    let saved = Tag::compound([("leash", Tag::compound([("UUID", Tag::IntArray(uuid.to_vec()))]))]);
    h.leash_load(cow, &saved);
    h.leash_resolve_pending();
    assert_eq!(leash::holder_of(h.world(), cow), None, "its holder is not there yet");
    let holder = h.spawn(mw, "minecraft:cow", [1.5, 64.0, 0.5], None).expect("a cow");
    h.set_uuid(holder, uuid);
    h.leash_resolve_pending();
    assert_eq!(leash::holder_of(h.world(), cow), Some(Holder::Entity(holder)));
}

#[test]
fn can_attach_is_false_beyond_the_snapping_distance() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let cow = h.spawn(mw, "minecraft:cow", [0.5, 64.0, 0.5], None).expect("a cow");
    assert!(h.leash_can_attach(cow, [0.5, 64.5, 0.5]), "a holder at the creature's side");
    assert!(!h.leash_can_attach(cow, [60.5, 64.5, 0.5]), "a holder sixty blocks away");
}

#[test]
fn entity_by_uuid_finds_the_entity_then_nothing_once_it_is_despawned() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let cow = h.spawn(mw, "minecraft:cow", [0.5, 64.0, 0.5], None).expect("a cow");
    let uuid = [5, 6, 7, 8];
    h.set_uuid(cow, uuid);
    assert_eq!(h.entity_by_uuid(uuid), Some(cow));
    h.despawn(cow);
    assert_eq!(h.entity_by_uuid(uuid), None);
}

#[test]
fn a_creature_view_is_a_creature_and_a_boat_view_is_a_vehicle() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let cow = h.spawn(mw, "minecraft:cow", [0.5, 64.0, 0.5], None).expect("a cow");
    let boat = h.world_mut().spawn((Pos(Vec3::new(10.5, 64.0, 10.5)), Vel(Vec3::ZERO), Boat { water_level: 64.0, ..Boat::default() })).id();
    let cv = view(h.world(), cow).expect("a view of the creature");
    assert!(cv.is_creature && !cv.is_vehicle);
    let bv = view(h.world(), boat).expect("a view of the boat");
    assert!(bv.is_vehicle && !bv.is_creature);
}
