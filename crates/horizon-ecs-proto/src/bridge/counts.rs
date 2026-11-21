//! The entity counts of a world, for the gauges and the reports: an entity is counted when it has a kind (`TypeName`). The total is
//! summed by archetype (no entity is visited); the counts by kind and the uuid check are the reports' and the transfer's, not a tick's.
use crate::bridge::uuids::entity_by_uuid;
use crate::cow::TypeName;
use bevy_ecs::prelude::*;
use std::collections::{BTreeMap, HashSet};

/// The number of entities with a kind, summed by archetype.
pub fn count(world: &World) -> usize {
    let Some(id) = world.component_id::<TypeName>() else { return 0 };
    world.archetypes().iter().filter(|a| a.contains(id)).map(|a| a.len() as usize).sum()
}

/// The entities with a kind, counted per kind: a scan of the world, for the reports only.
pub fn count_by_kind(world: &World) -> Vec<(&'static str, usize)> {
    let mut kinds = BTreeMap::<&'static str, usize>::new();
    for r in world.iter_entities() {
        if let Some(t) = r.get::<TypeName>() {
            *kinds.entry(t.0).or_default() += 1;
        }
    }
    kinds.into_iter().collect()
}

/// Whether a live entity holds one of `uuids`: one lookup in the uuid table per uuid.
pub fn holds_any_uuid(world: &World, uuids: &HashSet<[i32; 4]>) -> bool {
    uuids.iter().any(|&u| entity_by_uuid(world, u).is_some())
}
