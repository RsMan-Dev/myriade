//! Effect modifiers show in `attribute_instances` with the jar's ids, and a variant's armour base survives an attribute write.
use crate::common::*;
use horizon_client_lib::mobs::world::leaked_test_world_mut;
use horizon_ecs_proto::attr_mod::Operation;
use horizon_ecs_proto::behaviours::variant::set_variant;
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup};
use horizon_ecs_proto::effects::{self, Kind};
use horizon_ecs_proto::equipment::Armor;

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

#[test]
fn a_speed_effect_is_a_movement_speed_modifier_with_the_jar_id() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(effects::add(h.world_mut(), e, Kind::Speed, 1, 600));
    let rows = h.attribute_instances(e);
    let speed = rows.iter().find(|r| r.0 == "movement_speed").expect("a movement speed row");
    let m = speed.2.iter().find(|m| m.id == "minecraft:effect.speed").expect("the effect's modifier");
    assert_eq!((m.amount, m.operation), (0.4, Operation::AddMultipliedTotal));
    assert_eq!(h.drain_attribute_changes(), vec![e]);
}

#[test]
fn the_killer_rabbits_armour_survives_an_attribute_write() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let e = h.spawn(mw, "minecraft:rabbit", [0.5, 1.0, 0.5], None).expect("a known species");
    set_variant(h.world_mut(), e, 99);
    assert_eq!(h.world().get::<Armor>(e).map(|a| a.armor), Some(8.0));
    assert!(h.set_attribute_base(e, "minecraft:armor_toughness", 1.0));
    let a = h.world().get::<Armor>(e).expect("the armour");
    assert_eq!((a.armor, a.toughness), (8.0, 1.0));
}
