//! The custom name of a creature (`Entity.getCustomName`, keys `CustomName` and `CustomNameVisible`). Only a named creature carries
//! the component; the name is the JSON text component vanilla writes.
use bevy_ecs::prelude::*;

/// `CustomName`: the name as the text the host gave it (a player's rename, an egg's name).
#[derive(Component, Clone, Debug)]
#[component(storage = "SparseSet")]
pub struct CustomName(pub String);

/// `CustomNameVisible`: a marker, present only while the name shows above the creature.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct NameVisible;

/// Sets the name; `None` takes it away.
pub fn set(w: &mut World, e: Entity, name: Option<String>) {
    match name {
        Some(n) => w.entity_mut(e).insert(CustomName(n)),
        None => w.entity_mut(e).remove::<CustomName>(),
    };
}

pub fn get(w: &World, e: Entity) -> Option<String> {
    w.get::<CustomName>(e).map(|n| n.0.clone())
}

/// Sets the visibility; off removes the marker.
pub fn set_visible(w: &mut World, e: Entity, on: bool) {
    if on {
        w.entity_mut(e).insert(NameVisible);
    } else {
        w.entity_mut(e).remove::<NameVisible>();
    }
}
