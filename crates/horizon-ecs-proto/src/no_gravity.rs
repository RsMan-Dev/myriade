//! `Entity.setNoGravity` (key `NoGravity`): a marker, present only while the creature floats.
use bevy_ecs::prelude::*;

#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet", on_insert = crate::bridge::view::touched, on_remove = crate::bridge::view::touched)]
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

/// `Entity.getGravity`: the gravity of an entity that has one, none under `NoGravity` (the component is read through this, never alone).
pub fn gravity(w: &World, e: Entity) -> Option<f64> {
    if get(w, e) {
        return None;
    }
    w.get::<crate::cow::Gravity>(e).map(|g| g.0)
}

pub fn get(w: &World, e: Entity) -> bool {
    w.get::<NoGravity>(e).is_some()
}
