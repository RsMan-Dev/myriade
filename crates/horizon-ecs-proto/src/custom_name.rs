//! The custom name of a creature (`Entity.getCustomName`, keys `CustomName` and `CustomNameVisible`). Only a named creature carries
//! the component; the name is the JSON text component vanilla writes. A styled name (a name tag's `custom_name`, with a colour) keeps
//! its whole component in `NameComponent`, which the save writes back as it was given.
use bevy_ecs::prelude::*;
use horizon_core::nbt::{JavaString, Tag};

/// `CustomName`: the name as the text the host gave it (a player's rename, an egg's name).
#[derive(Component, Clone, Debug)]
#[component(storage = "SparseSet", on_insert = crate::bridge::view::touched, on_remove = crate::bridge::view::touched)]
pub struct CustomName(pub String);

/// The whole text component of a styled name (`ComponentSerialization.CODEC` writes a compound): kept as given, colour included.
#[derive(Component, Clone, Debug)]
#[component(storage = "SparseSet")]
pub struct NameComponent(pub Tag);

/// `Vindicator.extra.johnny`: latched once the name is Johnny, and kept by a rename (`setCustomName` only ever sets it). A load does not set it.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Johnny;

/// `CustomNameVisible`: a marker, present only while the name shows above the creature.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet", on_insert = crate::bridge::view::touched, on_remove = crate::bridge::view::touched)]
pub struct NameVisible;

/// Sets the plain name; `None` takes it away.
pub fn set(w: &mut World, e: Entity, name: Option<String>) {
    match name {
        Some(n) => {
            if n == "Johnny" {
                w.entity_mut(e).insert(Johnny);
            }
            w.entity_mut(e).insert(CustomName(n)).remove::<NameComponent>()
        }
        None => w.entity_mut(e).remove::<(CustomName, NameComponent)>(),
    };
    crate::bridge::view::mark(w, e);
}

/// Sets the whole component of a styled name (`NameTagItem.interactLivingEntity`); its plain text becomes the name.
pub fn set_component(w: &mut World, e: Entity, component: Tag) {
    let text = plain_text(&component);
    if text == "Johnny" {
        w.entity_mut(e).insert(Johnny);
    }
    w.entity_mut(e).insert((CustomName(text), NameComponent(component)));
    crate::bridge::view::mark(w, e);
}

pub fn get(w: &World, e: Entity) -> Option<String> {
    w.get::<CustomName>(e).map(|n| n.0.clone())
}

/// The name as the save writes it: the whole component when styled, else the plain text. `None` without a name.
pub fn tag(w: &World, e: Entity) -> Option<Tag> {
    match w.get::<NameComponent>(e) {
        Some(c) => Some(c.0.clone()),
        None => w.get::<CustomName>(e).map(|n| Tag::String(n.0.as_str().into())),
    }
}

/// Whether the saved `CustomName` is not this name, style included: a rename to the same text in another colour still changes it.
pub fn changed(w: &World, e: Entity, saved: Option<&Tag>) -> bool {
    match (w.get::<NameComponent>(e), saved) {
        (Some(current), Some(saved)) => *saved != current.0,
        (Some(_), None) => true,
        (None, _) => saved.map(plain_text) != get(w, e),
    }
}

/// The plain text of a component: a string, or the `text` of a compound followed by its `extra` parts.
pub fn plain_text(t: &Tag) -> String {
    match t {
        Tag::String(s) => s.to_string_lossy(),
        Tag::Compound(c) => {
            let mut out = match c.get(&JavaString::from("text")) {
                Some(Tag::String(s)) => s.to_string_lossy(),
                _ => String::new(),
            };
            if let Some(Tag::List(parts)) = c.get(&JavaString::from("extra")) {
                for p in parts {
                    out += &plain_text(p);
                }
            }
            out
        }
        _ => String::new(),
    }
}

/// Sets the visibility; off removes the marker.
pub fn set_visible(w: &mut World, e: Entity, on: bool) {
    if on {
        w.entity_mut(e).insert(NameVisible);
    } else {
        w.entity_mut(e).remove::<NameVisible>();
    }
    crate::bridge::view::mark(w, e);
}
