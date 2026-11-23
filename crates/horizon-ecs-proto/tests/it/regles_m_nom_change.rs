use crate::common::*;
use horizon_client_lib::mobs::world::leaked_test_world_mut;
use horizon_core::nbt::Tag;
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup};
use horizon_ecs_proto::custom_name;

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

/// `Entity.setCustomName` by a rename: a saved name that is not this one, style included, is a change; a plain name with the same text
/// (a mod's, or a styled one saved with the same text) is not.
#[test]
fn a_plain_name_matches_its_saved_text() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let cow = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    let w = h.world_mut();
    custom_name::set(w, cow, Some("Bob".into()));
    let plain = Tag::String("Bob".into());
    assert!(!custom_name::changed(w, cow, Some(&plain)), "same plain text");
    let styled = Tag::compound([("text", Tag::String("Bob".into())), ("color", Tag::String("red".into()))]);
    assert!(!custom_name::changed(w, cow, Some(&styled)), "same text, styled");
    assert!(custom_name::changed(w, cow, Some(&Tag::String("Alice".into()))), "another name");
    assert!(custom_name::changed(w, cow, None), "no saved name");
    custom_name::set_component(w, cow, styled.clone());
    assert!(!custom_name::changed(w, cow, Some(&styled)), "the component it holds");
    let restyled = Tag::compound([("text", Tag::String("Bob".into())), ("color", Tag::String("blue".into()))]);
    assert!(custom_name::changed(w, cow, Some(&restyled)), "a same-text rename in another colour");
}
