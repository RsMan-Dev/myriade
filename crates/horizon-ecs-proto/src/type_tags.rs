//! The tags of entity types (`data/<ns>/tags/entity_type`: vanilla's and the mods') as marker components: a tag is a zero-size component,
//! inserted on each entity of a species the tag names, at its creation (`mark`). `has` tests the archetype, `query` walks the entities of a
//! tag, and a tag the code reads on a hot path is a typed marker (`Typed`): systems then write `With<T>`. Component ids are per world, so a
//! `TagId` is an index into the process's names, and each world's `TypeTags` maps it to its component.
use crate::cow::{Species, SpeciesId};
use bevy_ecs::{
    component::{ComponentCloneBehavior, ComponentDescriptor, ComponentId, StorageType},
    prelude::*,
    ptr::OwningPtr,
    query::QueryBuilder,
};
use std::{ptr::NonNull, sync::RwLock};

/// An interned entity type tag: its index in the names (`MEMBERS`) and in each world's `TypeTags`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TagId(pub u16);

/// Each tag (`ns:name`, by `TagId`) and the species ids it names. Appended to by `install`, never reordered: a `TagId` stays a tag.
static MEMBERS: RwLock<Vec<(String, Vec<u8>)>> = RwLock::new(Vec::new());

/// A tag the code reads as a typed marker: `name` is the component `T`, registered in each world as the component of the tag.
#[derive(Clone, Copy)]
pub struct Typed {
    pub name: &'static str,
    register: fn(&mut World) -> ComponentId,
}

pub const fn typed<T: Component>(name: &'static str) -> Typed {
    Typed { name, register: register_typed::<T> }
}

fn register_typed<T: Component>(w: &mut World) -> ComponentId {
    w.register_component::<T>()
}

/// Per world: the component of each tag (by `TagId`), and the components of each species' tags (by `Species.id`), inserted at creation.
#[derive(Resource, Default)]
pub struct TypeTags {
    ids: Vec<ComponentId>,
    by_species: Vec<Vec<ComponentId>>,
    typed: &'static [Typed],
}

/// Loads every entity type tag of `root` (members flattened by the loader) against `species`, replacing the memberships of the names
/// already known. The species table's ids are what the memberships hold.
pub fn install(root: &std::path::Path, species: &[Species]) {
    let raw = myriade_engine_lib::items::load_tags(root, "entity_type", &horizon_world::loot::PackOverlay::NONE).expect("entity_type tags");
    let mut names: Vec<&String> = raw.keys().collect();
    names.sort();
    let mut members = MEMBERS.write().unwrap();
    for (_, m) in members.iter_mut() {
        m.clear();
    }
    for name in names {
        let ids: Vec<u8> = species.iter().filter(|s| raw[name].contains(s.name)).map(|s| s.id).collect();
        match members.iter().position(|(n, _)| n == name) {
            Some(i) => members[i].1 = ids,
            None => members.push((name.clone(), ids)),
        }
    }
}

/// Declares a tag at run time (a mod's, without a data file) as the species `members` name, and gives it to `w`: the entities made after
/// this call with those species get its marker. A name already known has its members replaced.
pub fn declare(w: &mut World, name: &str, members: &[u8]) {
    {
        let mut all = MEMBERS.write().unwrap();
        match all.iter().position(|(n, _)| n == name) {
            Some(i) => all[i].1 = members.to_vec(),
            None => all.push((name.to_owned(), members.to_vec())),
        }
    }
    sync(w);
}

/// The id of a tag, given as `undead`, `#undead`, `minecraft:undead` or a mod's `ns:name`; None when no tag has that name.
pub fn id(name: &str) -> Option<TagId> {
    let name = name.trim_start_matches('#');
    let key = if name.contains(':') { name.to_owned() } else { format!("minecraft:{name}") };
    MEMBERS.read().unwrap().iter().position(|(n, _)| *n == key).map(|i| TagId(i as u16))
}

/// Whether the tag names the species (by its `Species.id`), for the cold paths that have no entity yet (a test's setup).
pub fn lists(name: &str, species: u8) -> bool {
    id(name).is_some_and(|t| MEMBERS.read().unwrap()[usize::from(t.0)].1.contains(&species))
}

/// Gives a world its tags: each name is a component (a `typed` one is the component of its type), and each entity made later with a
/// species gets the markers of its tags (`mark`). Called once by the world's builder, after `install`.
pub fn register(w: &mut World, typed: &'static [Typed]) {
    w.insert_resource(TypeTags { typed, ..Default::default() });
    sync(w);
}

/// Registers the components of the names this world does not know yet, and rebuilds the species' lists.
fn sync(w: &mut World) {
    let names: Vec<(String, Vec<u8>)> = MEMBERS.read().unwrap().clone();
    let typed = w.resource::<TypeTags>().typed;
    let mut ids = std::mem::take(&mut w.resource_mut::<TypeTags>().ids);
    for (name, _) in names.iter().skip(ids.len()) {
        let id = match typed.iter().find(|t| t.name == name.strip_prefix("minecraft:").unwrap_or(name)) {
            Some(t) => (t.register)(w),
            // SAFETY: a zero-size, table-stored component with no drop and no value: nothing to read or drop.
            None => w.register_component_with_descriptor(unsafe {
                ComponentDescriptor::new_with_layout(
                    name.clone(),
                    StorageType::Table,
                    std::alloc::Layout::new::<()>(),
                    None,
                    false,
                    false,
                    ComponentCloneBehavior::default(),
                    None,
                )
            }),
        };
        ids.push(id);
    }
    let len = names.iter().flat_map(|(_, m)| m).max().map_or(0, |&m| usize::from(m) + 1);
    let mut by_species = vec![Vec::new(); len];
    for ((_, members), &id) in names.iter().zip(&ids) {
        for &s in members {
            by_species[usize::from(s)].push(id);
        }
    }
    let mut t = w.resource_mut::<TypeTags>();
    t.ids = ids;
    t.by_species = by_species;
}

/// Re-reads the tags (a `/reload`): `install` the new memberships, register the new names, and move the markers of every entity with a
/// species to the new lists. Linear in the entities: a rare event, not a tick's work.
pub fn reload(w: &mut World, root: &std::path::Path, species: &[Species]) {
    install(root, species);
    sync(w);
    let all: Vec<ComponentId> = w.resource::<TypeTags>().ids.clone();
    let holders: Vec<(Entity, u8)> = w.query::<(Entity, &SpeciesId)>().iter(w).map(|(e, s)| (e, s.0)).collect();
    for (e, s) in holders {
        let held: Vec<ComponentId> = all.iter().copied().filter(|&c| w.entity(e).contains_id(c)).collect();
        if !held.is_empty() {
            w.entity_mut(e).remove_by_ids(&held);
        }
        mark(w, e, s);
    }
}

/// Inserts the markers of the species' tags on `e`, in one move of its archetype. Called at the creation of an entity with a `SpeciesId`.
pub fn mark(w: &mut World, e: Entity, species: u8) {
    // `Entity.isIgnoringBlockTriggers`: a capability of the species, read once here (no name is compared in a system).
    let ignores = w.get_resource::<crate::cow::SpeciesTable>().and_then(|t| t.0.get(usize::from(species)).map(|s| s.name)).and_then(|n| crate::species::registry().by_name(n).ok()).is_some_and(|r| r.caps.contains(&crate::behaviours::Cap::IgnoresTriggers));
    if ignores {
        w.entity_mut(e).insert(crate::inside::IgnoresTriggers);
    }
    let piston_ignored = w.get_resource::<crate::cow::SpeciesTable>().and_then(|t| t.0.get(usize::from(species)).map(|s| s.name)).and_then(|n| crate::species::registry().by_name(n).ok()).is_some_and(|r| r.caps.contains(&crate::behaviours::Cap::PistonIgnored));
    if piston_ignored {
        w.entity_mut(e).insert(crate::world_push::PistonIgnored);
    }
    // `Monster.getFallSounds`: a monster's falls sound hostile.
    let monster = w.get_resource::<crate::cow::SpeciesTable>().and_then(|t| t.0.get(usize::from(species)).map(|s| s.name)).and_then(|n| crate::species::registry().by_name(n).ok()).is_some_and(|r| r.spawn.is_some_and(|s| matches!(s.category, crate::spawning::Category::Monster)));
    if monster {
        w.entity_mut(e).insert(crate::landing_trample::HostileFall);
    }
    if lists("minecraft:can_float_while_ridden", species) {
        w.entity_mut(e).insert(crate::cow::FloatsWhileRidden);
    }
    // `Mob.controlsVehicle`: a rider with this tag does not steer its vehicle.
    if lists("minecraft:non_controlling_rider", species) {
        w.entity_mut(e).insert(crate::mount::NonControllingRider);
    }
    let caps = w.get_resource::<crate::cow::SpeciesTable>().and_then(|t| t.0.get(usize::from(species)).map(|s| s.name)).and_then(|n| crate::species::registry().by_name(n).ok()).map(|r| r.caps);
    if caps.is_some_and(|c| c.contains(&crate::behaviours::Cap::Collidable)) {
        w.entity_mut(e).insert(crate::world_push::Collidable);
    }
    if caps.is_some_and(|c| c.contains(&crate::behaviours::Cap::CollidableWhenStill)) {
        w.entity_mut(e).insert(crate::world_push::CollidableWhenStill);
    }
    if caps.is_some_and(|c| c.contains(&crate::behaviours::Cap::RefusedByMobs)) {
        w.entity_mut(e).insert(crate::targeting::RefusedByMobs);
    }
    let no_berry = w.get_resource::<crate::cow::SpeciesTable>().and_then(|t| t.0.get(usize::from(species)).map(|s| s.name)).and_then(|n| crate::species::registry().by_name(n).ok()).is_some_and(|r| r.caps.contains(&crate::behaviours::Cap::IgnoresBerryBush));
    if no_berry {
        w.entity_mut(e).insert(crate::inside_effects::IgnoresBerryBush);
    }
    let never = caps.and_then(|c| {
        let bits = if c.contains(&crate::behaviours::Cap::NeverStuck) { crate::inside_effects::NeverStuck::ALL } else if c.contains(&crate::behaviours::Cap::NoWebStick) { crate::inside_effects::NeverStuck::WEBS } else { 0 };
        (bits != 0).then_some(crate::inside_effects::NeverStuck(bits))
    });
    if let Some(n) = never {
        w.entity_mut(e).insert(n);
    }
    let Some(list) = w.get_resource_mut::<TypeTags>().and_then(|mut t| t.by_species.get_mut(usize::from(species)).map(std::mem::take)) else {
        return;
    };
    if list.is_empty() {
        return;
    }
    // SAFETY: each id is a zero-size component of this world, so any non-null pointer is a valid value of it.
    unsafe {
        w.entity_mut(e).insert_by_ids(&list, list.iter().map(|_| OwningPtr::new(NonNull::<u8>::dangling())));
    }
    if let Some(mut t) = w.get_resource_mut::<TypeTags>() {
        t.by_species[usize::from(species)] = list;
    }
}

/// Whether the entity is in the tag (no tag of that id in this world, or a dead entity: false).
pub fn has(w: &World, tag: TagId, e: Entity) -> bool {
    let Some(c) = w.get_resource::<TypeTags>().and_then(|t| t.ids.get(usize::from(tag.0)).copied()) else { return false };
    w.get_entity(e).is_ok_and(|r| r.contains_id(c))
}

/// `has` by name, for the cold paths and the tests (the name is resolved at each call).
pub fn named(w: &World, name: &str, e: Entity) -> bool {
    id(name).is_some_and(|t| has(w, t, e))
}

/// Whether the entity is in the typed tag `T` (the component a `Typed` names).
pub fn is<T: Component>(w: &World, e: Entity) -> bool {
    w.get_entity(e).is_ok_and(|r| r.contains::<T>())
}

/// The entities of a tag, as a query (`With`-style filters of the tag's component are its own: this is the tag alone).
pub fn query(w: &mut World, tag: TagId) -> QueryState<Entity> {
    let c = w.resource::<TypeTags>().ids[usize::from(tag.0)];
    let mut b = QueryBuilder::<Entity, ()>::new(w);
    b.with_id(c);
    b.build()
}
