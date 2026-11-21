//! `Entity.setNoGravity` (key `NoGravity`): a marker, present only while the creature floats.
use bevy_ecs::prelude::*;

#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct NoGravity;

/// Sets the flag; off removes the marker.
pub fn set(w: &mut World, e: Entity, on: bool) {
    if on {
        w.entity_mut(e).insert(NoGravity);
    } else {
        w.entity_mut(e).remove::<NoGravity>();
    }
    crate::bridge::view::mark(w, e);
}

pub fn get(w: &World, e: Entity) -> bool {
    w.get::<NoGravity>(e).is_some()
}
