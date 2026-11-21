//! The reads of a living entity's state through the host world (greffe L0, lot 42): effects, flags, attributes, pose and equipment,
//! read back after the writes that set them.
use crate::common::*;
use horizon_client_lib::items::ItemStack;
use horizon_client_lib::mobs::world::leaked_test_world_mut;
use horizon_ecs_proto::attributes::set_armor_modifier;
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup};
use horizon_ecs_proto::cow::{Body, MaxHealth, set_on_fire};
use horizon_ecs_proto::effects::{self, Kind};
use horizon_ecs_proto::bridge::host_read_living::{effect_kind, effect_name};
use horizon_ecs_proto::goals::set_main_hand;

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

#[test]
fn effect_names_resolve_both_ways() {
    assert_eq!(effect_kind("minecraft:poison"), Some(Kind::Poison));
    assert_eq!(effect_kind("minecraft:not_an_effect"), None);
    assert_eq!(effect_name(Kind::Poison), "minecraft:poison");
}

#[test]
fn effect_read_back_with_its_amplifier_and_a_step_less_remaining() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(effects::add(h.world_mut(), e, Kind::Poison, 1, 100));
    assert_eq!(h.effect(e, Kind::Poison).map(|x| x.amp), Some(1));
    assert!(h.has_effect(e, Kind::Poison));
    let before = h.effect_remaining(e, Kind::Poison).expect("the effect is there");
    h.step(mw);
    assert_eq!(h.effect_remaining(e, Kind::Poison), Some(before - 1));
}

#[test]
fn on_fire_flag_follows_set_on_fire() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert_eq!(h.flag(e, "on_fire"), Some(false));
    set_on_fire(h.world_mut(), e, 100);
    assert_eq!(h.flag(e, "on_fire"), Some(true));
    assert_eq!(h.flag(e, "not_a_flag"), None);
}

#[test]
fn attributes_read_the_components_and_the_armour_modifiers() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    let max = h.world().get::<MaxHealth>(e).expect("a max health").0;
    assert_eq!(h.attribute(e, "minecraft:max_health"), Some(f64::from(max)));
    assert!(h.attribute_modifiers(e, "minecraft:armor").is_empty());
    set_armor_modifier(h.world_mut(), e, "test:armour", Some(2.0));
    let mods = h.attribute_modifiers(e, "minecraft:armor");
    assert_eq!(mods.iter().map(|m| (m.id.as_str(), m.amount)).collect::<Vec<_>>(), vec![("test:armour", 2.0)]);
}

#[test]
fn standing_pose_is_the_body_size() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    let b = *h.world().get::<Body>(e).expect("a body");
    let (w, hgt, _) = h.pose_dimensions(e, 0).expect("the standing pose");
    assert_eq!((w, hgt), (b.width, b.height));
    assert_eq!(h.pose_dimensions(e, 1), None);
}

#[test]
fn main_hand_stack_is_the_one_set() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    let stack = ItemStack { item: "minecraft:stone".into(), count: 1, damage: 0, extra: None };
    set_main_hand(h.world_mut(), e, Some(stack.clone()));
    assert_eq!(h.equipment_stack(e, 0), Some(stack));
}
