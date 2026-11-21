//! The entity type tags read by `type_tags` against the original's loader (`items::load_tags` over `entity_type`, members flattened): for
//! each tag and each species of the table, the same membership. And the effects that read them: a harming splash heals a zombie
//! (`inverted_healing_and_harm`) and hurts a cow.
use crate::common::*;
use horizon_ecs_proto::{
    cow::{spawn_species, species_table, DamageQueue, Health},
    effects::{self, Brew},
    type_tags::{self, TypeTags},
};
use horizon_core::geometry::Vec3;

/// The original's members of a tag (`ns:name`, nested `#tag` already flattened by the loader).
fn original(name: &str) -> std::collections::HashSet<String> {
    let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let tags = horizon_client_lib::items::load_tags(&root, "entity_type", &horizon_world::loot::PackOverlay::NONE).expect("entity_type tags");
    tags.get(&format!("minecraft:{name}")).cloned().unwrap_or_default()
}

#[test]
fn entity_type_tags_are_the_original_s_for_every_species() {
    if !have_data() {
        return;
    }
    let (_, h) = flat_pair();
    let tags = h.world.resource::<TypeTags>();
    let table = species_table();
    for name in ["undead", "inverted_healing_and_harm", "ignores_poison_and_regen", "arthropod", "powder_snow_walkable_mobs", "skeletons"] {
        let tag = type_tags::id(name).expect(name);
        let members = original(name);
        for sp in &table.0 {
            assert_eq!(tags.has(tag, sp.id), members.contains(sp.name), "{name} for {}", sp.name);
        }
    }
}

#[test]
fn a_nested_tag_is_flattened_into_the_tag_naming_it() {
    if !have_data() {
        return;
    }
    let (_, h) = flat_pair();
    let tags = h.world.resource::<TypeTags>();
    let table = species_table();
    let undead = original("undead");
    assert!(!undead.is_empty());
    let ignores = type_tags::id("ignores_poison_and_regen").expect("ignores_poison_and_regen");
    for sp in table.0.iter().filter(|sp| undead.contains(sp.name)) {
        assert!(tags.has(ignores, sp.id), "{} is undead, so it is in ignores_poison_and_regen", sp.name);
    }
}

#[test]
fn an_unknown_tag_has_no_id_and_the_name_forms_agree() {
    if !have_data() {
        return;
    }
    let _ = flat_pair();
    assert_eq!(type_tags::id("no_such_entity_tag"), None);
    assert_eq!(type_tags::id("undead"), type_tags::id("#minecraft:undead"));
    assert_eq!(type_tags::id("undead"), type_tags::id("minecraft:undead"));
    assert!(type_tags::id("undead").is_some());
}

#[test]
fn a_harming_splash_heals_a_zombie_and_hurts_a_cow() {
    if !have_data() {
        return;
    }
    let (_, mut h) = flat_pair();
    let cow = ecs_cow(&mut h, 0.5, 1.0, 0.5, 5);
    let zombie_sp = species_table().0.into_iter().find(|s| s.name == "minecraft:zombie").expect("the zombie species");
    let zombie = spawn_species(&mut h.world, &zombie_sp, 4.5, 4.5, 7);
    h.world.get_mut::<Health>(zombie).unwrap().0 = 5.0;
    h.tick();
    effects::splash(&mut h.world, Vec3::new(4.5, 1.0, 4.5), Some(zombie), &[Brew::Harm(0)], None);
    assert_eq!(h.world.get::<Health>(zombie).unwrap().0, 11.0, "a zombie is healed by harming: 6 whole");
    effects::splash(&mut h.world, Vec3::new(0.5, 1.0, 0.5), Some(cow), &[Brew::Harm(0)], None);
    let hits: Vec<f32> = h.world.resource::<DamageQueue>().0.iter().filter(|x| x.target == cow).map(|x| x.amount).collect();
    assert_eq!(hits, vec![6.0], "a cow is hurt by harming: 6 whole");
}
