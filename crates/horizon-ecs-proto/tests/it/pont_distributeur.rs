//! The dispenser's effects through the bridge: shearing, the TNT fuse of a dispensed flame, brushing and an egg's name.
use crate::common::*;
use horizon_client_lib::items::ItemStack;
use horizon_client_lib::mobs::world::leaked_test_world_mut;
use horizon_core::nbt::Tag;
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup};
use horizon_ecs_proto::cow::now_of;
use horizon_ecs_proto::dispense::{Brushable, Shearable};
use horizon_ecs_proto::tnt::{FUSE, Primed};

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

#[test]
fn shearing_takes_once_and_only_a_shearable_creature() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let plain = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(!h.set_sheared(plain, true), "a creature without Shearable is not shorn");
    assert!(!h.is_sheared(plain));
    let e = h.spawn(mw, "minecraft:cow", [2.5, 1.0, 0.5], None).expect("a known species");
    h.world_mut().entity_mut(e).insert(Shearable);
    assert!(h.set_sheared(e, true), "the first shearing takes");
    assert!(!h.set_sheared(e, true), "a second shearing changes nothing");
    assert!(h.is_sheared(e));
}

#[test]
fn prime_lights_a_fuse_of_the_tnt_length_once() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(h.prime(e, None));
    let lit = *h.world().get::<Primed>(e).expect("the entity is lit");
    assert_eq!(lit.fuse_at - now_of(h.world(), e), FUSE as u64, "the fuse is the TNT's 80 ticks");
    assert!(!h.prime(e, None), "a lit entity is not lit again");
}

#[test]
fn brush_drops_from_the_table() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(h.brush(e).is_empty(), "a creature without Brushable keeps its drop");
    h.world_mut().entity_mut(e).insert(Brushable);
    let drops = h.brush(e);
    assert!(!drops.is_empty(), "the brush table drops a stack");
}

#[test]
fn an_egg_with_a_name_names_the_creature() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    let mut egg = ItemStack::new("minecraft:cow_spawn_egg", 1);
    egg.data_mut().custom_name = Some(Tag::String("Blade".into()));
    h.apply_egg_components(e, &egg, None, None);
    assert_eq!(h.custom_name(e).as_deref(), Some("Blade"));
}
