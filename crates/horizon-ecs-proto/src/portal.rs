//! The portals of the ECS entities (lot 73): who may take one, the box an entity has when it enters a
//! portal. Nothing here scans the world: the entries are read by
//! `EcsWorld::portal_entries`, which visits only the entities that moved (`Moving`).
use crate::cow::{Body, Dying, Health};
use crate::mount::Riding;
use crate::seat::Seated;
use bevy_ecs::prelude::*;
use horizon_core::dimension::Dimension;

/// Where a portal sends an entity: the dimension (the host's own), the place and the yaw of the arrival.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PortalTarget {
    pub dimension: Dimension,
    pub at: [f64; 3],
    pub yaw: f32,
}

/// An entity that takes no portal (a falling block, an item of a player): set by the species' configuration.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct NoPortal;

/// `Entity.canUsePortal(allowVehicles)` (jar): alive and not `NoPortal`; a passenger only when `allow_vehicles` is set.
pub fn usable(w: &World, e: Entity, allow_vehicles: bool) -> bool {
    let alive = w.get::<Health>(e).is_none_or(|h| h.0 > 0.0) && w.get::<Dying>(e).is_none();
    let passenger = w.get::<Seated>(e).is_some() || w.get::<Riding>(e).is_some();
    alive && w.get::<NoPortal>(e).is_none() && (allow_vehicles || !passenger)
}

/// The box of an entity (`Body`): its width and height, as the portal reads them.
pub fn size_of(w: &World, e: Entity) -> Option<(f32, f32)> {
    w.get::<Body>(e).map(|b| (b.width, b.height))
}
