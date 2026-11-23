//! The entity type tags, as marker components, against the original's loader (`items::load_tags` over `entity_type`, members flattened):
//! for each tag and each species of the table, the entity of that species is in the tag exactly when the original lists it. A query
//! of a tag gives those entities alone. An entity with no species has no marker. And the effects that read them: a harming splash heals
//! a zombie (`inverted_healing_and_harm`) and hurts a cow.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{
    cow::{spawn_species, species_table, DamageQueue, Health, Herd, Species},
    effects::{self, Brew},
    type_tags,
};

/// The original's members of a tag (`ns:name`, nested `#tag` already flattened by the loader).
fn original(name: &str) -> std::collections::HashSet<String> {
    let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let tags = horizon_client_lib::items::load_tags(&root, "entity_type", &horizon_world::loot::PackOverlay::NONE).expect("entity_type tags");
    tags.get(&format!("minecraft:{name}")).cloned().unwrap_or_default()
}

/// One entity of each species of the table, at its own place.
fn one_of_each(h: &mut Herd) -> Vec<(Entity, Species)> {
    species_table().0.into_iter().enumerate().map(|(i, sp)| (spawn_species(&mut h.world, &sp, 0.5 + 4.0 * i as f64, 0.5, 9 + i as u64), sp)).collect()
}

const TAGS: [&str; 6] = ["undead", "inverted_healing_and_harm", "ignores_poison_and_regen", "arthropod", "powder_snow_walkable_mobs", "skeletons"];

#[test]
fn entity_type_tags_are_the_original_s_for_every_species() {
    if !have_data() {
        return;
    }
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    let made = one_of_each(&mut h);
    for name in TAGS {
        let tag = type_tags::id(name).expect(name);
        let members = original(name);
        for (e, sp) in &made {
            assert_eq!(type_tags::has(&h.world, tag, *e), members.contains(sp.name), "{name} for {}", sp.name);
        }
    }
}

#[test]
fn a_query_of_a_tag_gives_exactly_the_entities_of_its_members() {
    if !have_data() {
        return;
    }
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    let made = one_of_each(&mut h);
    let members = original("undead");
    let mut expected: Vec<Entity> = made.iter().filter(|(_, sp)| members.contains(sp.name)).map(|(e, _)| *e).collect();
    let mut got: Vec<Entity> = {
        let mut q = type_tags::query(&mut h.world, type_tags::id("undead").expect("undead"));
        q.iter(&h.world).collect()
    };
    expected.sort();
    got.sort();
    assert!(!expected.is_empty());
    assert_eq!(got, expected);
}

#[test]
fn a_nested_tag_is_flattened_into_the_tag_naming_it() {
    if !have_data() {
        return;
    }
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    let made = one_of_each(&mut h);
    let undead = original("undead");
    assert!(!undead.is_empty());
    let ignores = type_tags::id("ignores_poison_and_regen").expect("ignores_poison_and_regen");
    for (e, sp) in made.iter().filter(|(_, sp)| undead.contains(sp.name)) {
        assert!(type_tags::has(&h.world, ignores, *e), "{} is undead, so it is in ignores_poison_and_regen", sp.name);
    }
}

#[test]
fn an_entity_without_a_species_has_no_marker_and_an_unknown_tag_has_no_id() {
    if !have_data() {
        return;
    }
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    let bare = h.world.spawn_empty().id();
    for name in TAGS {
        assert!(!type_tags::has(&h.world, type_tags::id(name).expect(name), bare), "{name}");
    }
    assert!(!type_tags::is::<effects::IgnoresPoisonAndRegen>(&h.world, bare));
    assert_eq!(type_tags::id("no_such_entity_tag"), None);
    assert_eq!(type_tags::id("undead"), type_tags::id("#minecraft:undead"));
    assert_eq!(type_tags::id("undead"), type_tags::id("minecraft:undead"));
}

#[test]
fn a_harming_splash_heals_a_zombie_and_hurts_a_cow() {
    if !have_data() {
        return;
    }
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    let cow = ecs_cow(&mut h, 0.5, 1.0, 0.5, 5);
    if horizon_ecs_proto::type_tags::named(&h.world, "undead", cow) {
        let t = std::thread::current();
        horizon_ecs_proto::cow::skip_note(t.name().unwrap_or("?"), "the species under test is undead: harming heals it (`LivingEntity.isInvertedHealAndHarm`, jar)");
        return;
    }
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
