//! The secondary blocks of the professions, as the bridge resolves them at load: a mod's profession keeps its declared blocks, and a
//! vanilla profession gains the members of its `horizon:villager_secondary_poi/<name>` tag (the farmer's farmland is already in its entry).
use crate::mobs::villager::ResolvedProfessions;
use std::collections::{HashMap, HashSet};

fn id(s: &str) -> mod_api::Identifier {
    mod_api::Identifier::parse(s).unwrap()
}

fn engineer(secondary: &str) -> mod_api::ProfessionDefinition {
    mod_api::ProfessionDefinition {
        key: mod_api::ProfessionKey(id("test:engineer")),
        workstations: vec![],
        held_poi: vec![],
        acquirable_poi: vec![],
        requested_items: vec![],
        secondary_blocks: vec![id(secondary)],
        work_sound: None,
        trades: vec![],
    }
}

/// A mod's profession (index 15) keeps the block its declaration names, and the vanilla farmer keeps its farmland.
#[test]
fn a_mod_profession_keeps_its_declared_secondary_blocks() {
    let table = ResolvedProfessions::prepare(vec![("test".into(), engineer("minecraft:stone"))], |_, _| true).unwrap();
    let mod_entry = table.entry(15).expect("the mod profession");
    assert!(mod_entry.secondary_blocks.contains("minecraft:stone"));
    assert!(table.entry(5).unwrap().secondary_blocks.contains("minecraft:farmland"));
}

/// The tag's members join their vanilla profession only: the farmer gains soul sand, the fisherman its cauldron, the cleric (no member) stays empty.
#[test]
fn a_vanilla_profession_gains_the_members_of_its_tag_once() {
    let mut table = ResolvedProfessions::vanilla();
    let mut tags: HashMap<String, HashSet<String>> = HashMap::new();
    tags.insert("horizon:villager_secondary_poi/farmer".into(), HashSet::from(["minecraft:soul_sand".to_string()]));
    tags.insert("horizon:villager_secondary_poi/fisherman".into(), HashSet::from(["minecraft:water_cauldron".to_string()]));
    table.add_secondary_tags(&tags);
    let farmer = &table.entry(5).unwrap().secondary_blocks;
    assert!(farmer.contains("minecraft:soul_sand") && farmer.contains("minecraft:farmland"));
    assert!(table.entry(6).unwrap().secondary_blocks.contains("minecraft:water_cauldron"), "the fisherman's tag is its entry");
    assert!(table.entry(4).unwrap().secondary_blocks.is_empty(), "a profession without a tag member keeps an empty entry: no scan");
    let before = table.entry(5).unwrap().secondary_blocks.len();
    table.add_secondary_tags(&tags);
    assert_eq!(table.entry(5).unwrap().secondary_blocks.len(), before, "a second load adds nothing");
}

/// A mod block whose template is farmland (its class is `FarmlandBlock`) joins the farmer's set, the one the harvest reads as soil; a
/// non-farmland mod block does not.
#[test]
fn mod_farmland_is_a_secondary_site_and_farming_target() {
    let mut table = ResolvedProfessions::vanilla();
    let mut tags: HashMap<String, HashSet<String>> = HashMap::new();
    tags.insert("horizon:class/farmland".into(), HashSet::from(["test:rich_soil".to_string()]));
    table.add_secondary_tags(&tags);
    let farmer = &table.entry(5).unwrap().secondary_blocks;
    assert!(farmer.contains("test:rich_soil"), "a mod farmland block is a secondary site of the farmer");
    assert!(farmer.contains("minecraft:farmland"), "the vanilla farmland stays");
    assert!(table.entry(6).unwrap().secondary_blocks.is_empty(), "the farmland class is the farmer's alone");
}
