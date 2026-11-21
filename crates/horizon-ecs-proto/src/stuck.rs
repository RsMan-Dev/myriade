//! The arrows and stingers stuck in a creature (`arrowCount`, `stingerCount`). Vanilla writes no NBT key for them, so they are not saved.
use bevy_ecs::prelude::*;

#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Stuck {
    pub arrows: u8,
    pub stingers: u8,
}

/// Sets both counts; two zeros remove the component.
pub fn set(w: &mut World, e: Entity, arrows: u8, stingers: u8) {
    if arrows == 0 && stingers == 0 {
        w.entity_mut(e).remove::<Stuck>();
    } else {
        w.entity_mut(e).insert(Stuck { arrows, stingers });
    }
}

/// `(arrows, stingers)`; `(0, 0)` without the component.
pub fn get(w: &World, e: Entity) -> (u8, u8) {
    w.get::<Stuck>(e).map_or((0, 0), |s| (s.arrows, s.stingers))
}
