//! The tags of entity types (`data/<ns>/tags/entity_type`: vanilla's and the mods'), read by id. `install` loads every tag once against the
//! species table and keeps, per tag, the set of species it names (`SpeciesSet`); `has` is one bit test on the entity's `SpeciesId`. A name
//! resolves once with `id` (a constant, never per entity or per tick).
use crate::cow::{Species, SpeciesId, SpeciesSet};
use bevy_ecs::prelude::*;
use std::sync::OnceLock;

/// An interned entity type tag: its index in `TypeTags` and in the names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TagId(pub u16);

/// The names of the tags (`ns:name`), by `TagId`, and the resource built from them: both set once, by `install`.
static NAMES: OnceLock<Vec<String>> = OnceLock::new();
static TAGS: OnceLock<TypeTags> = OnceLock::new();

/// Per tag (by `TagId`), the species it names.
#[derive(Resource, Clone, Default)]
pub struct TypeTags(Vec<SpeciesSet>);

impl TypeTags {
    /// Whether the tag names the species of id `species`.
    pub fn has(&self, tag: TagId, species: u8) -> bool {
        self.0.get(usize::from(tag.0)).is_some_and(|s| s.has(species))
    }
}

/// Loads every entity type tag of the test resources (`HORIZON_TEST_RESOURCES`), members flattened by the loader, and resolves each
/// against `species` (the species whose name a tag lists). A second call keeps the first, as the species table does.
pub fn install(species: &[Species]) {
    let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let raw = horizon_client_lib::items::load_tags(&root, "entity_type", &horizon_world::loot::PackOverlay::NONE).expect("entity_type tags");
    let mut names: Vec<&String> = raw.keys().collect();
    names.sort();
    let sets = names
        .iter()
        .map(|name| species.iter().filter(|s| raw[*name].contains(s.name)).fold(SpeciesSet::EMPTY, |set, s| set.with(usize::from(s.id))))
        .collect();
    let _ = NAMES.set(names.into_iter().cloned().collect());
    let _ = TAGS.set(TypeTags(sets));
}

/// The tags of the process (see `install`), for the resource of a world.
pub fn table() -> TypeTags {
    TAGS.get().expect("a level was built").clone()
}

/// The id of a tag, given as `undead`, `#undead`, `minecraft:undead` or a mod's `ns:name`; None when no tag has that name.
pub fn id(name: &str) -> Option<TagId> {
    let name = name.trim_start_matches('#');
    let key = if name.contains(':') { name.to_owned() } else { format!("minecraft:{name}") };
    NAMES.get()?.iter().position(|n| *n == key).map(|i| TagId(i as u16))
}

/// Whether the entity is of a type the tag names (false for an entity with no species, or without tags loaded).
pub fn has(w: &World, tag: TagId, e: Entity) -> bool {
    w.get_resource::<TypeTags>().is_some_and(|t| w.get::<SpeciesId>(e).is_some_and(|s| t.has(tag, s.0)))
}

/// `has` by name, for the cold paths and the tests (the name is resolved at each call).
pub fn named(w: &World, name: &str, e: Entity) -> bool {
    id(name).is_some_and(|t| has(w, t, e))
}
