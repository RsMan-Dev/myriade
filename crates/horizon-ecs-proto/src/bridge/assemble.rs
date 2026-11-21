//! The world of a host: the resources and the schedule's tables of a herd (`cow::assemble_resources`), over a level with no blocks of its
//! own. Nothing is spawned here: a host world starts empty, its creatures come from the host's own calls.
use crate::cow::{Level, assemble_resources};
use bevy_ecs::prelude::*;

/// An empty host world over `level`, read by the schedule's systems while it runs. The level is taken out again by the caller, so it
/// never outlives the step that lent it.
pub(crate) fn assemble(level: Level) -> World {
    let mut world = World::new();
    assemble_resources(&mut world, level, true, false);
    world
}
