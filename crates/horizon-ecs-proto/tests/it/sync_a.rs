//! The species' synched entries (greffe L0, lot 05): each slot of a species' row gives the entry the Mob sends for the component
//! it reads, with the values written out from vanilla's formulas, and nothing at the default.
use bevy_ecs::prelude::*;
use horizon_ecs_proto::behaviours::{beg::Begging, convert_timer::Converting, variant::Variant};
use horizon_ecs_proto::bridge::{sync::SyncValue, sync_a};
use horizon_ecs_proto::farm::Wool;
use horizon_ecs_proto::size::Size;

/// The entries the row of `kind` gives for `e`, in slot order.
fn entries(w: &World, kind: &str, e: Entity) -> Vec<(u8, SyncValue)> {
    let (_, slots) = sync_a::TABLE.iter().find(|(k, _)| *k == kind).expect("a species row");
    slots.iter().filter_map(|s| (s.read)(w, e).map(|v| (s.index, v))).collect()
}

#[test]
fn sheep_wool_is_the_colour_with_16_when_sheared_and_nothing_at_the_default() {
    let mut w = World::new();
    let plain = w.spawn(Wool { sheared: false, color: 0 }).id();
    let dyed = w.spawn(Wool { sheared: false, color: 3 }).id();
    let shorn = w.spawn(Wool { sheared: true, color: 3 }).id();
    let bare = w.spawn(Wool { sheared: true, color: 0 }).id();
    assert!(entries(&w, "minecraft:sheep", plain).is_empty(), "white and whole is the default");
    assert_eq!(entries(&w, "minecraft:sheep", dyed), [(18, SyncValue::Byte(3))]);
    assert_eq!(entries(&w, "minecraft:sheep", shorn), [(18, SyncValue::Byte(19))]);
    assert_eq!(entries(&w, "minecraft:sheep", bare), [(18, SyncValue::Byte(16))]);
}

#[test]
fn cube_size_is_sent_above_one_for_each_cube_species() {
    let mut w = World::new();
    let small = w.spawn(Size(1)).id();
    let big = w.spawn(Size(4)).id();
    for kind in ["minecraft:slime", "minecraft:magma_cube", "minecraft:sulfur_cube"] {
        assert!(entries(&w, kind, small).is_empty(), "size 1 is the default");
        assert_eq!(entries(&w, kind, big), [(18, SyncValue::Int(4))], "{kind}");
    }
}

#[test]
fn fox_type_is_sent_when_not_red() {
    let mut w = World::new();
    let red = w.spawn(Variant(0)).id();
    let snow = w.spawn(Variant(1)).id();
    assert!(entries(&w, "minecraft:fox", red).is_empty(), "red is the default");
    assert_eq!(entries(&w, "minecraft:fox", snow), [(18, SyncValue::Int(1))]);
}

#[test]
fn zombie_villager_shakes_while_its_conversion_runs() {
    let mut w = World::new();
    let curing = w.spawn(Converting { until: 500 }).id();
    let idle = w.spawn_empty().id();
    assert!(entries(&w, "minecraft:zombie_villager", idle).is_empty());
    assert_eq!(entries(&w, "minecraft:zombie_villager", curing), [(19, SyncValue::Bool(true))]);
}

#[test]
fn wolf_is_interested_while_it_begs() {
    let mut w = World::new();
    let begging = w.spawn(Begging { player: 1, ticks: 10 }).id();
    let calm = w.spawn_empty().id();
    assert!(entries(&w, "minecraft:wolf", calm).is_empty());
    assert_eq!(entries(&w, "minecraft:wolf", begging), [(20, SyncValue::Bool(true))]);
}
