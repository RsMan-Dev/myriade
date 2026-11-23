use crate::common::*;
use horizon_client_lib::items::ItemStack;
use horizon_client_lib::mobs::world::leaked_test_world_mut;
use horizon_core::nbt::Tag;
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup};
use horizon_ecs_proto::goals::Slot;
use horizon_ecs_proto::item::Item;
use horizon_ecs_proto::item_store::ItemStore;
use horizon_ecs_proto::loot;
use horizon_ecs_proto::mount::Mount;
use horizon_ecs_proto::mount_inventory;
use horizon_ecs_proto::spawn_gear;

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

/// `Mob.dropCustomDeathLoot` spawns the saddle as it is: a named saddle drops with its name.
#[test]
fn a_named_saddle_drops_with_its_name() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let horse = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    h.world_mut().entity_mut(horse).insert(Mount { max_temper: 1, ..Default::default() });
    let mut saddle = ItemStack::new("minecraft:saddle", 1);
    saddle.data_mut().custom_name = Some(Tag::String("Seat".into()));
    h.mount_store(horse, Some(saddle), None, vec![]);
    let drops = mount_inventory::death_stacks(h.world_mut(), horse);
    assert_eq!(drops[0].custom_name().as_deref(), Some("Seat"));
    assert_eq!((&*drops[0].item, drops[0].count), ("minecraft:saddle", 1));
}

/// `AbstractChestedHorse.dropEquipment`: the chest's stack that a vanishing curse prevents stays with the mount, the others drop.
#[test]
fn a_donkey_keeps_its_vanishing_cursed_chest_stack() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let donkey = h.spawn(mw, "minecraft:donkey", [0.5, 1.0, 0.5], None).expect("a known species");
    h.world_mut().entity_mut(donkey).insert(Mount { max_temper: 1, ..Default::default() });
    let mut cursed = ItemStack::new("minecraft:emerald", 2);
    cursed.enchant("minecraft:vanishing_curse", 1);
    mount_inventory::chest_on(h.world_mut(), donkey, 5);
    h.mount_store(donkey, None, None, vec![Some(ItemStack::new("minecraft:diamond", 4)), Some(cursed)]);
    loot::on_death(h.world_mut(), donkey);
    let world = h.world_mut();
    let mut q = world.query::<&Item>();
    let items: Vec<String> = q.iter(world).map(|i| i.stack.item.to_string()).collect();
    assert!(items.iter().any(|n| n == "minecraft:diamond"), "{items:?}");
    assert!(!items.iter().any(|n| n == "minecraft:emerald"), "{items:?}");
}

/// `Wolf` wears its armor with a drop chance of 2: it drops for certain, and with its name.
#[test]
fn a_worn_wolf_armor_drops_with_its_name() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let wolf = h.spawn(mw, "minecraft:wolf", [0.5, 1.0, 0.5], None).expect("a known species");
    let mut armor = ItemStack::new("minecraft:wolf_armor", 1);
    armor.data_mut().custom_name = Some(Tag::String("Fang".into()));
    h.mount_store(wolf, None, Some(Some(armor)), vec![]);
    spawn_gear::set_drop_chance(h.world_mut(), wolf, Slot::Body, 2.0);
    loot::on_death(h.world_mut(), wolf);
    let world = h.world_mut();
    let mut q = world.query::<&Item>();
    let armors: Vec<Option<String>> = q.iter(world).filter(|i| &*i.stack.item == "minecraft:wolf_armor").map(|i| i.stack.custom_name().map(|n| n.to_string())).collect();
    assert_eq!(armors, [Some("Fang".to_owned())]);
}

/// `Piglin.dropCustomDeathLoot`: each stack of its eight-slot inventory drops whole.
#[test]
fn a_piglin_drops_every_stack_of_its_inventory() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let piglin = h.spawn(mw, "minecraft:piglin", [0.5, 1.0, 0.5], None).expect("a known species");
    h.world_mut().get_mut::<ItemStore>(piglin).expect("an inventory").items[7] = Some(ItemStack::new("minecraft:gold_ingot", 2));
    h.world_mut().get_mut::<ItemStore>(piglin).expect("an inventory").items[0] = Some(ItemStack::new("minecraft:diamond", 4));
    loot::on_death(h.world_mut(), piglin);
    let world = h.world_mut();
    let mut q = world.query::<&Item>();
    let items: Vec<(String, u32)> = q.iter(world).map(|i| (i.stack.item.to_string(), i.stack.count as u32)).collect();
    assert!(items.contains(&("minecraft:diamond".to_owned(), 4)), "{items:?}");
    assert!(items.contains(&("minecraft:gold_ingot".to_owned(), 2)), "{items:?}");
}
