use crate::common::*;
use myriade_engine_lib::mobs::world::leaked_test_world_mut;
use horizon_core::nbt::{JavaString, Tag};
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup};
use horizon_ecs_proto::custom_name;
use horizon_ecs_proto::nbt;

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

fn name_of(c: &horizon_core::nbt::Compound) -> Option<Tag> {
    c.get(&JavaString::from("CustomName")).cloned()
}

/// `NameTagItem.interactLivingEntity` sets the tag's whole component on the creature, and `Entity.saveWithoutId` writes that component
/// back: a styled name keeps its colour through a save and a restore; a plain name is saved as plain text.
#[test]
fn a_named_mobs_custom_name_saves_its_whole_component() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let cow = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    let styled = Tag::compound([("text", Tag::String("Bob".into())), ("color", Tag::String("red".into()))]);
    custom_name::set_component(h.world_mut(), cow, styled.clone());
    assert_eq!(custom_name::get(h.world(), cow).as_deref(), Some("Bob"));
    let saved = nbt::save(h.world(), cow).expect("a saved creature");
    assert_eq!(name_of(&saved), Some(styled.clone()), "the colour survives the save");
    let back = nbt::restore(h.world_mut(), &saved).expect("a declared, saved type");
    assert_eq!(custom_name::tag(h.world(), back), Some(styled.clone()), "the restore names it the same way");

    custom_name::set(h.world_mut(), cow, Some("Bob".into()));
    let plain = nbt::save(h.world(), cow).expect("a saved creature");
    assert_eq!(name_of(&plain), Some(Tag::String("Bob".into())), "a plain name is plain text");
}
