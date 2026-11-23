//! `Entity.isSprinting` (`setSprinting`, the sprint bit of the shared flags): a marker, absent when the entity does not sprint.
use bevy_ecs::prelude::*;

/// The entity sprints: a walk of a creature with `WalkPaces` at the sprint speed. Written and removed by `walk_paces::pace`.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Sprinting;
