//! The visible state read by the bridge's generic slots and species rows (greffe L0, lot 48): baby, sitting and tame flags, size,
//! fuse. Each entry gives its vanilla value, and nothing at the default.
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::behaviours::posture::Posture;
use horizon_ecs_proto::bridge::sync::{SyncValue, sync_entries};
use horizon_ecs_proto::bridge::uuids::EntityUuid;
use horizon_ecs_proto::bridge::{sync_a, sync_b, sync_core};
use horizon_ecs_proto::cow::{GrowAt, Pos, TypeName, Vel};
use horizon_ecs_proto::explosion::Fuse;
use horizon_ecs_proto::size::Size;
use horizon_ecs_proto::tame::{Owner, set_owner};
use horizon_ecs_proto::targeting::Who;

/// The entry of `index` that the generic slots give `e` now.
fn core(index: u8, w: &World, e: Entity) -> Option<SyncValue> {
    sync_core::SLOTS.iter().find(|s| s.index == index).and_then(|s| (s.read)(w, e))
}

/// The entry of `index` that the species' table gives `e` now.
fn read(table: &[(&str, &[horizon_ecs_proto::bridge::sync::SyncSlot])], kind: &str, index: u8, w: &World, e: Entity) -> Option<SyncValue> {
    let (_, slots) = table.iter().find(|(k, _)| *k == kind).expect("a table for the species");
    slots.iter().find(|s| s.index == index).and_then(|s| (s.read)(w, e))
}

#[test]
fn an_adult_sends_no_baby_entry_and_a_baby_sends_true_at_16() {
    let mut w = World::new();
    let adult = w.spawn_empty().id();
    let baby = w.spawn(GrowAt(100)).id();
    assert_eq!(core(16, &w, adult), None, "an adult is the default");
    assert_eq!(core(16, &w, baby), Some(SyncValue::Bool(true)));
}

#[test]
fn a_tamed_sitting_wolf_sends_the_flags_byte_0x05() {
    let mut w = World::new();
    let e = w.spawn((Owner(Some(Who::Player(1))), Posture { sitting: true, since: 0, sit_down: 0, stand_up: 0 })).id();
    assert_eq!(read(sync_a::TABLE, "minecraft:wolf", 18, &w, e), Some(SyncValue::Byte(5)));
    let standing = w.spawn(Owner(Some(Who::Player(1)))).id();
    assert_eq!(read(sync_a::TABLE, "minecraft:wolf", 18, &w, standing), Some(SyncValue::Byte(4)));
    let wild = w.spawn_empty().id();
    assert_eq!(read(sync_a::TABLE, "minecraft:wolf", 18, &w, wild), None, "untamed and standing is the default");
}

#[test]
fn a_pet_sends_the_uuid_of_its_owner_entity() {
    let mut w = World::new();
    w.init_resource::<horizon_ecs_proto::bridge::uuids::Uuids>();
    let owner = w.spawn(EntityUuid(42)).id();
    let pet = w.spawn_empty().id();
    set_owner(&mut w, pet, Who::Entity(owner));
    assert_eq!(read(sync_a::TABLE, "minecraft:wolf", 19, &w, pet), Some(SyncValue::OptUuid(Some(42))));
}

#[test]
fn set_owner_is_the_owner_the_tamer_gave() {
    let mut w = World::new();
    w.init_resource::<horizon_ecs_proto::tame::Pets>();
    let e = w.spawn_empty().id();
    set_owner(&mut w, e, Who::Player(7));
    assert!(matches!(w.get::<Owner>(e), Some(Owner(Some(Who::Player(7))))));
}

#[test]
fn a_cube_of_size_three_sends_three() {
    let mut w = World::new();
    let e = w.spawn(Size(3)).id();
    assert_eq!(read(sync_a::TABLE, "minecraft:slime", 18, &w, e), Some(SyncValue::Int(3)));
}

#[test]
fn a_lit_creeper_sends_its_fuse_direction_and_the_ignited_flag() {
    let mut w = World::new();
    let e = w.spawn(Fuse { value: 0, at: 0, dir: 1, max: 30, radius: 3.0, armed: 0, ignited: true }).id();
    assert_eq!(read(sync_b::TABLE, "minecraft:creeper", 16, &w, e), Some(SyncValue::Int(1)));
    assert_eq!(read(sync_b::TABLE, "minecraft:creeper", 18, &w, e), Some(SyncValue::Bool(true)));
}

#[test]
fn a_species_without_these_components_sends_the_same_entries_as_before() {
    let mut w = World::new();
    let e = w.spawn((Pos(Vec3::new(0.5, 1.0, 0.5)), Vel(Vec3::ZERO), TypeName("minecraft:cat"))).id();
    let mut out = Vec::new();
    sync_entries(&w, e, &mut out);
    assert!(out.is_empty(), "a cat with no state sends nothing: the default");
}
