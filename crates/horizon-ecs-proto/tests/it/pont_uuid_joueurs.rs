//! The uuids of the players on the bridge (greffe lot 82): a joined player's uuid is the one the save writes; a tamed animal, a TNT and a
//! lead of a player are written by it and read back; an owner read before its player joined waits for the connection. Written, not run.
use crate::common::*;
use bevy_ecs::prelude::*;
use myriade_engine_lib::mobs::world::leaked_test_world_mut;
use myriade_engine_lib::mobs::MobWorld;
use horizon_core::nbt::{Compound, Tag};
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup};
use horizon_ecs_proto::nbt;
use horizon_ecs_proto::player_uuids::uuid_of;
use horizon_ecs_proto::tame::Owner;
use horizon_ecs_proto::targeting::Who;

const UUID: [i32; 4] = [0x1234_5678, -0x0765_4321, 0x0BAD_F00D, 0x7F00_0001];

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

/// A host world with one wolf at the origin, and the world it reads blocks from.
fn with_wolf() -> (&'static MobWorld<'static>, EcsWorld, Entity) {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let e = h.spawn(mw, "minecraft:wolf", [0.5, 64.0, 0.5], None).expect("a known species");
    (mw, h, e)
}

#[test]
fn a_joined_player_keeps_the_uuid_the_save_writes() {
    let (_, mut h, _) = with_wolf();
    h.player_joined(7, UUID);
    assert_eq!(h.player_uuid(7), Some(UUID));
    assert_eq!(h.player_uuid(8), None);
}

#[test]
fn a_tamed_animal_of_a_joined_player_saves_the_player_uuid_as_owner() {
    let (_, mut h, e) = with_wolf();
    h.player_joined(7, UUID);
    h.world_mut().entity_mut(e).insert(Owner(Some(Who::Player(7))));
    assert_eq!(nbt::get(h.world(), e, "Owner"), Some(Tag::IntArray(UUID.to_vec())));
}

#[test]
fn an_owner_read_before_its_player_joined_waits_and_is_set_at_the_connection() {
    let (_, mut h, e) = with_wolf();
    let mut c = Compound::new();
    c.insert("Owner".into(), Tag::IntArray(UUID.to_vec()));
    assert!(nbt::load(h.world_mut(), e, &c).is_some());
    assert_eq!(h.world().get::<Owner>(e).map(|o| o.0), Some(None));
    h.player_joined(7, UUID);
    assert_eq!(h.world().get::<Owner>(e).map(|o| o.0), Some(Some(Who::Player(7))));
}

#[test]
fn a_connection_empties_the_wait_so_a_second_connection_does_not_change_the_owner() {
    let (_, mut h, e) = with_wolf();
    let mut c = Compound::new();
    c.insert("Owner".into(), Tag::IntArray(UUID.to_vec()));
    assert!(nbt::load(h.world_mut(), e, &c).is_some());
    h.player_joined(7, UUID);
    h.player_joined(9, UUID);
    assert_eq!(h.world().get::<Owner>(e).map(|o| o.0), Some(Some(Who::Player(7))));
}

#[test]
fn a_primed_tnt_of_a_joined_player_saves_the_player_uuid_as_owner() {
    let (mw, mut h, _) = with_wolf();
    h.player_joined(7, UUID);
    let e = h.prime_tnt(mw, [4.5, 64.0, 4.5], 80, Some(Who::Player(7)), 4.0);
    assert_eq!(nbt::get(h.world(), e, "owner"), Some(Tag::IntArray(UUID.to_vec())));
}

#[test]
fn a_lead_held_by_a_player_names_its_uuid_once_joined() {
    let (_, mut h, _) = with_wolf();
    h.player_joined(7, UUID);
    assert_eq!(uuid_of(h.world(), 7), Some(0x1234_5678_F89A_BCDF_0BAD_F00D_7F00_0001));
}
