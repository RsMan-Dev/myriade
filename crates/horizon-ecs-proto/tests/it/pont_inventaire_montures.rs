//! The chest of a mount through the bridge (greffe lot 80): a horse has a menu with no column until a chest goes on, the chest's slots come
//! back as written (enchantments kept) and are cut or padded to `columns * 3`, and a read writes nothing. Needs `HORIZON_TEST_RESOURCES`,
//! as the other bridge tests.
use crate::common::*;
use myriade_engine_lib::items::ItemStack;
use myriade_engine_lib::mobs::world::leaked_test_world_mut;
use horizon_core::nbt::Tag;
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup};
use horizon_ecs_proto::goals::Equipment;
use horizon_ecs_proto::mount::Mount;
use horizon_ecs_proto::behaviours::forget_target::LlamaStrength;
use horizon_ecs_proto::mount_inventory::{self, MountInventory};

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

/// An enchanted sword with `damage` points of wear, the enchantments as a stand-in tag.
fn sword(damage: u32) -> ItemStack {
    let mut s = ItemStack::new("minecraft:diamond_sword", 1);
    s.damage = damage;
    s.data_mut().set_other("minecraft:enchantments", Some(Tag::String("minecraft:sharpness=5".into())));
    s
}

#[test]
fn a_creature_without_a_seat_has_no_menu() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let cow = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(h.mount_columns(cow).is_none());
    assert!(h.mount_contents(cow).is_none());
}

#[test]
fn a_mount_without_a_chest_has_a_menu_with_no_column() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let cow = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    h.world_mut().entity_mut(cow).insert(Mount { max_temper: 1, ..Default::default() });
    assert_eq!(h.mount_columns(cow).map(|c| c.columns), Some(0));
    assert_eq!(h.mount_contents(cow).map(|c| c.items.len()), Some(0));
}

#[test]
fn the_chest_items_come_back_as_written() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let horse = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    h.world_mut().entity_mut(horse).insert(Mount { max_temper: 1, ..Default::default() });
    mount_inventory::chest_on(h.world_mut(), horse, 5);
    let items = vec![Some(sword(0)), None, Some(sword(3))];
    h.mount_store(horse, None, None, items.clone());
    assert_eq!(h.mount_columns(horse).map(|c| c.columns), Some(5));
    let back = h.mount_contents(horse).expect("a mount").items;
    assert_eq!(back.len(), 15);
    assert_eq!(back[..3], items[..]);
    assert!(back[3..].iter().all(Option::is_none));
}

#[test]
fn two_columns_make_six_slots() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let horse = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    h.world_mut().entity_mut(horse).insert(Mount { max_temper: 1, ..Default::default() });
    mount_inventory::chest_on(h.world_mut(), horse, 2);
    h.mount_store(horse, None, None, vec![Some(sword(0)); 9]);
    assert_eq!(h.mount_contents(horse).map(|c| c.items.len()), Some(6));
}

#[test]
fn a_read_writes_no_component() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let cow = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(h.mount_columns(cow).is_none());
    assert!(h.mount_contents(cow).is_none());
    assert!(h.world().get::<MountInventory>(cow).is_none());
    assert!(h.world().get::<Equipment>(cow).is_none());
}

#[test]
fn a_horse_leaves_its_saddle_its_items_and_its_chest() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let horse = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    h.world_mut().entity_mut(horse).insert(Mount { max_temper: 1, ..Default::default() });
    mount_inventory::chest_on(h.world_mut(), horse, 5);
    h.mount_store(horse, Some(ItemStack::new("minecraft:saddle", 1)), None, vec![Some(sword(2))]);
    let drops = mount_inventory::death_stacks(h.world_mut(), horse);
    assert_eq!(drops.len(), 3);
    assert_eq!(drops[1], sword(2));
    assert_eq!(drops[2], ItemStack::new("minecraft:chest", 1));
    assert!(h.mount_contents(horse).expect("a mount").saddle.is_none());
    assert!(h.world().get::<MountInventory>(horse).is_none());
}

#[test]
fn a_llama_chest_has_as_many_columns_as_its_strength() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let llama = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    h.world_mut().entity_mut(llama).insert((Mount { max_temper: 1, ..Default::default() }, LlamaStrength(3)));
    let columns = mount_inventory::chest_columns(h.world(), llama, 5);
    mount_inventory::chest_on(h.world_mut(), llama, columns);
    assert_eq!(h.mount_columns(llama).map(|c| c.columns), Some(3));
    assert_eq!(h.mount_contents(llama).map(|c| c.items.len()), Some(9));
}
