//! In-game entity tags (`/tag`, the NBT `Tags` list, the selector `tag=`): one zero-size marker component per tag name, registered on its
//! first use and stored sparse, so an entity pays for the tags it carries and nothing else. `names` walks the entity's archetype, so no
//! list is stored. Vanilla caps an entity at 1024 tags (`Entity.addTag`).
use bevy_ecs::{
    component::{ComponentCloneBehavior, ComponentDescriptor, ComponentId, StorageType},
    prelude::*,
    ptr::OwningPtr,
    query::QueryBuilder,
};
use crate::rules::nbt_tag::tag_names;
use horizon_core::nbt::{Compound, JavaString, Tag};
use std::{collections::HashMap, ptr::NonNull};

/// Vanilla's cap on the tags of one entity: `Entity.addTag` refuses the 1025th.
pub const MAX_TAGS: usize = 1024;

/// The marker of each tag name in this world, and the reverse map that reads an entity's names off its archetype.
#[derive(Resource, Default)]
struct NbtTags {
    ids: HashMap<String, ComponentId>,
    names: HashMap<ComponentId, String>,
}

/// The marker of a tag name, registered on first use.
fn marker(w: &mut World, name: &str) -> ComponentId {
    if let Some(c) = w.get_resource_or_init::<NbtTags>().ids.get(name).copied() {
        return c;
    }
    // SAFETY: a zero-size, sparse-stored component with no drop and no value: nothing to read or drop.
    let c = w.register_component_with_descriptor(unsafe {
        ComponentDescriptor::new_with_layout(
            name.to_owned(),
            StorageType::SparseSet,
            std::alloc::Layout::new::<()>(),
            None,
            false,
            false,
            ComponentCloneBehavior::default(),
            None,
        )
    });
    let mut t = w.resource_mut::<NbtTags>();
    t.ids.insert(name.to_owned(), c);
    t.names.insert(c, name.to_owned());
    c
}

/// The names of the tags an entity carries, sorted (vanilla's set has no order).
pub fn names(w: &World, e: Entity) -> Vec<&str> {
    let (Some(t), Ok(r)) = (w.get_resource::<NbtTags>(), w.get_entity(e)) else { return Vec::new() };
    let mut v: Vec<&str> = r.archetype().iter_components().filter_map(|c| t.names.get(&c).map(String::as_str)).collect();
    v.sort_unstable();
    v
}

/// Whether the entity carries the tag (a tag never named in this world, or a dead entity: false).
pub fn has(w: &World, e: Entity, name: &str) -> bool {
    let Some(c) = w.get_resource::<NbtTags>().and_then(|t| t.ids.get(name).copied()) else { return false };
    w.get_entity(e).is_ok_and(|r| r.contains_id(c))
}

/// Adds the tag. False when the entity already has it, has `MAX_TAGS` tags, is dead, or is not `Saved` (its tags are its NBT's:
/// vanilla's `addTag`).
pub fn add(w: &mut World, e: Entity, name: &str) -> bool {
    if w.get::<crate::nbt::Saved>(e).is_none() {
        return false;
    }
    add_unsaved(w, e, name)
}
/// Adds the tag to an entity that is not `Saved` either (a leash knot read from a summon's data: vanilla reads `Tags` for every entity).
pub fn add_unsaved(w: &mut World, e: Entity, name: &str) -> bool {
    if w.get_entity(e).is_err() || has(w, e, name) || names(w, e).len() >= MAX_TAGS {
        return false;
    }
    let c = marker(w, name);
    // SAFETY: the id is a zero-size component of this world, so any non-null pointer is a valid value of it.
    unsafe {
        w.entity_mut(e).insert_by_ids(&[c], std::iter::once(OwningPtr::new(NonNull::<u8>::dangling())));
    }
    true
}

/// Removes every tag of the entity (it lost its NBT).
pub fn clear(w: &mut World, e: Entity) {
    let carried: Vec<String> = names(w, e).into_iter().map(String::from).collect();
    for name in carried {
        remove(w, e, &name);
    }
}

/// Removes the tag. False when the entity did not carry it.
pub fn remove(w: &mut World, e: Entity, name: &str) -> bool {
    if !has(w, e, name) {
        return false;
    }
    let c = w.resource::<NbtTags>().ids[name];
    w.entity_mut(e).remove_by_id(c);
    true
}

/// The entities that carry the tag, alone.
pub fn query(w: &mut World, name: &str) -> QueryState<Entity> {
    let c = marker(w, name);
    let mut b = QueryBuilder::<Entity, ()>::new(w);
    b.with_id(c);
    b.build()
}

/// Adds the `Tags` list of the entity to its compound; no key when it has no tag.
pub fn write(w: &World, e: Entity, out: &mut Compound) {
    let list: Vec<Tag> = names(w, e).into_iter().map(|n| Tag::String(JavaString::from(n))).collect();
    if !list.is_empty() {
        out.insert(JavaString::from("Tags"), Tag::List(list));
    }
}

/// Replaces the tags of the entity by the `Tags` list. False, and nothing changes, when the tag is not a list of strings. Names past
/// `MAX_TAGS` are dropped, as `addTag` drops them.
pub fn read(w: &mut World, e: Entity, tag: &Tag) -> bool {
    let Some(new) = tag_names(tag) else { return false };
    for old in names(w, e).into_iter().map(str::to_owned).collect::<Vec<_>>() {
        remove(w, e, &old);
    }
    for name in &new {
        add(w, e, name);
    }
    true
}
