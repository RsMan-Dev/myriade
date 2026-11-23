//! The entity type tags as marker components, read by name: for five vanilla tags (`illager`, `raiders`, `zombies`, `skeletons`,
//! `aquatic`), the entity of each species carries the marker exactly when the original's tag lists that species. A tag declared at run
//! time (a mod's) is posed on the entities made after its declaration, and on no other.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_ecs_proto::{
    cow::{spawn_species, species_table, Herd, Species},
    type_tags,
};

/// The original's members of a tag (`ns:name`, nested `#tag` already flattened by the loader).
fn original(name: &str) -> std::collections::HashSet<String> {
    let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let tags = horizon_client_lib::items::load_tags(&root, "entity_type", &horizon_world::loot::PackOverlay::NONE).expect("entity_type tags");
    tags.get(&format!("minecraft:{name}")).cloned().unwrap_or_default()
}

const TAGS: [&str; 5] = ["illager", "raiders", "zombies", "skeletons", "aquatic"];

#[test]
fn vanilla_tags_mark_exactly_the_species_they_list() {
    if !have_data() {
        return;
    }
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    let made: Vec<(Entity, Species)> =
        species_table().0.into_iter().enumerate().map(|(i, sp)| (spawn_species(&mut h.world, &sp, 0.5 + 4.0 * i as f64, 0.5, 9 + i as u64), sp)).collect();
    for name in TAGS {
        let members = original(name);
        assert!(!members.is_empty(), "{name} lists no species in the original");
        for (e, sp) in &made {
            assert_eq!(type_tags::named(&h.world, name, *e), members.contains(sp.name), "{name} for {}", sp.name);
        }
    }
}

#[test]
fn a_tag_declared_at_run_time_is_posed_at_creation() {
    if !have_data() {
        return;
    }
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    let table = species_table().0;
    let cow = *table.iter().find(|s| s.name == "minecraft:cow").expect("the cow species");
    let zombie = *table.iter().find(|s| s.name == "minecraft:zombie").expect("the zombie species");
    type_tags::declare(&mut h.world, "horizon_test:pets", &[cow.id]);
    let c = spawn_species(&mut h.world, &cow, 0.5, 0.5, 3);
    let z = spawn_species(&mut h.world, &zombie, 4.5, 0.5, 4);
    assert!(type_tags::named(&h.world, "horizon_test:pets", c), "the cow is a member");
    assert!(!type_tags::named(&h.world, "horizon_test:pets", z), "the zombie is not");
}
