//! `Mob.setLeftHanded` (key `LeftHanded`): a marker, present only on a left-handed creature; absent means right-handed.
use bevy_ecs::prelude::*;

#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct LeftHanded;

/// Sets the hand; off removes the marker.
pub fn set(w: &mut World, e: Entity, on: bool) {
    if on {
        w.entity_mut(e).insert(LeftHanded);
    } else {
        w.entity_mut(e).remove::<LeftHanded>();
    }
}

pub fn get(w: &World, e: Entity) -> bool {
    w.get::<LeftHanded>(e).is_some()
}
