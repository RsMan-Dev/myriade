//! The host's writes of absorption and attribute modifiers (lot 75): the components are sparse and the value follows vanilla's order.
use crate::common::*;
use bevy_ecs::entity::Entity;
use horizon_client_lib::mobs::world::leaked_test_world_mut;
use horizon_ecs_proto::absorption::{self, Absorption};
use horizon_ecs_proto::attr_mod::{AttrMods, Modifier, Operation, value};
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup};

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

fn world_and_cow() -> (EcsWorld, Entity) {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    (h, e)
}

fn modifier(id: &str, amount: f64, operation: Operation) -> Modifier {
    Modifier { id: id.to_owned(), amount, operation, permanent: false }
}

#[test]
fn a_new_entity_has_no_absorption_nor_modifiers() {
    let (h, e) = world_and_cow();
    assert!(h.world().get::<Absorption>(e).is_none());
    assert!(h.world().get::<AttrMods>(e).is_none());
    assert_eq!(h.absorption(e), 0.0);
}

#[test]
fn absorption_round_trips_and_zero_removes_it() {
    let (mut h, e) = world_and_cow();
    h.set_absorption(e, 4.0);
    assert_eq!(h.absorption(e), 4.0);
    h.set_absorption(e, 0.0);
    assert!(h.world().get::<Absorption>(e).is_none(), "0 removes the component");
    assert_eq!(h.absorption(e), 0.0);
}

#[test]
fn absorb_takes_the_absorption_before_the_health() {
    let (mut h, e) = world_and_cow();
    h.set_absorption(e, 4.0);
    let rest = absorption::absorb(h.world_mut(), e, 3.0);
    assert_eq!(rest, 0.0, "the absorption covers the whole hit");
    assert_eq!(h.absorption(e), 1.0, "one heart of absorption is left");
}

#[test]
fn modifiers_apply_in_vanilla_order() {
    let (mut h, e) = world_and_cow();
    assert!(h.add_attribute_modifier(e, "minecraft:attack_damage", modifier("test:add", 2.0, Operation::AddValue)));
    assert!(h.add_attribute_modifier(e, "minecraft:attack_damage", modifier("test:total", 0.5, Operation::AddMultipliedTotal)));
    let mods = h.attribute_modifiers(e, "minecraft:attack_damage");
    assert_eq!(mods.len(), 2);
    // (10 + 2) * (1 + 0.5), the total multiplier applying after the added value
    assert_eq!(value(10.0, &mods), 18.0);
}

#[test]
fn a_modifier_is_removed_once() {
    let (mut h, e) = world_and_cow();
    assert!(h.add_attribute_modifier(e, "minecraft:attack_damage", modifier("test:add", 2.0, Operation::AddValue)));
    assert!(h.remove_attribute_modifier(e, "minecraft:attack_damage", "test:add"));
    assert!(!h.remove_attribute_modifier(e, "minecraft:attack_damage", "test:add"));
    assert!(h.world().get::<AttrMods>(e).is_none(), "an empty list leaves no component");
}
