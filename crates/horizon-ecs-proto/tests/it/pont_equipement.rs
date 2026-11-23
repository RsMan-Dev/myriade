//! The equipment stacks through the bridge (greffe lot 76): a whole stack per slot, read back as written, an event on a change only, the
//! component gone with the last piece, and a piece that wears to zero leaves its slot. Needs `HORIZON_TEST_RESOURCES`, as the other bridge tests.
use crate::common::*;
use myriade_engine_lib::items::ItemStack;
use myriade_engine_lib::mobs::world::leaked_test_world_mut;
use horizon_core::nbt::Tag;
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup};
use horizon_ecs_proto::equip_stack::{self, Equipment};
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::host_out_equipment::EquipmentEvent;

/// The slot changes the channel carries since the last step: (entity, slot, from, to).
fn changes(h: &mut EcsWorld, mw: &myriade_engine_lib::mobs::MobWorld<'_>) -> (Vec<(bevy_ecs::entity::Entity, usize, Option<ItemStack>, Option<ItemStack>)>, Vec<(bevy_ecs::entity::Entity, usize)>) {
    let (mut changed, mut broke) = (Vec::new(), Vec::new());
    for ev in h.step(mw) {
        match ev {
            HostEvent::Equipment(EquipmentEvent::Changed { entity, slot, from, to }) => changed.push((entity, slot, from, to)),
            HostEvent::Equipment(EquipmentEvent::Broke { entity, slot }) => broke.push((entity, slot)),
            _ => {}
        }
    }
    (changed, broke)
}
use horizon_ecs_proto::equipment_items::durability_of;
use horizon_ecs_proto::items::item_id;

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

/// An enchanted sword with `damage` points of wear. The enchantments are a stand-in tag: the piece keeps its components whole.
fn sword(damage: u32) -> ItemStack {
    let mut s = ItemStack::new("minecraft:diamond_sword", 1);
    s.damage = damage;
    s.data_mut().set_other("minecraft:enchantments", Some(Tag::String("minecraft:sharpness=5".into())));
    s
}

#[test]
fn a_new_entity_wears_nothing() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let cow = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(h.equipment_stack(cow, 0).is_none());
    assert!(h.world().get::<Equipment>(cow).is_none());
    assert!(changes(&mut h, mw).0.is_empty());
}

#[test]
fn the_stack_read_back_keeps_its_enchantments_and_wear() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let cow = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(h.set_equipment_stack(cow, 0, sword(5)));
    assert_eq!(h.equipment_stack(cow, 0), Some(sword(5)));
}

#[test]
fn writing_the_same_stack_again_queues_nothing() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let cow = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(h.set_equipment_stack(cow, 0, sword(0)));
    let seen = changes(&mut h, mw).0;
    assert_eq!(seen.len(), 1);
    assert_eq!((seen[0].0, seen[0].1), (cow, 0));
    assert!(h.set_equipment_stack(cow, 0, sword(0)));
    assert!(changes(&mut h, mw).0.is_empty());
    assert!(changes(&mut h, mw).0.is_empty());
}

#[test]
fn emptying_every_slot_removes_the_component() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let cow = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(h.set_equipment_stack(cow, 0, sword(0)));
    assert!(equip_stack::set(h.world_mut(), cow, 0, None));
    assert!(h.world().get::<Equipment>(cow).is_none());
    assert!(h.equipment_stack(cow, 0).is_none());
}

#[test]
fn wear_to_zero_breaks_the_piece_and_queues_the_break() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let cow = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    let max = durability_of(item_id("minecraft:diamond_sword"));
    assert!(max > 0);
    assert!(h.set_equipment_stack(cow, 0, sword(0)));
    changes(&mut h, mw);
    assert!(equip_stack::damage(h.world_mut(), cow, 0, max));
    assert!(h.equipment_stack(cow, 0).is_none());
    let (seen, broke) = changes(&mut h, mw);
    assert_eq!(broke, vec![(cow, 0)]);
    assert_eq!(seen.len(), 1);
    assert_eq!((seen[0].1, seen[0].2.clone(), seen[0].3.clone()), (0, Some(sword(0)), None));
    assert!(changes(&mut h, mw).1.is_empty());
}

#[test]
fn equip_sound_is_the_items_own_or_the_generic_one() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let cow = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert_eq!(h.equip_sound(cow, 0, None).as_deref(), Some("minecraft:item.armor.equip_generic"));
    assert_eq!(h.equip_sound(cow, 2, Some("minecraft:item.armor.equip_gold".into())).as_deref(), Some("minecraft:item.armor.equip_gold"));
    assert!(h.equip_sound(cow, 8, None).is_none());
}
