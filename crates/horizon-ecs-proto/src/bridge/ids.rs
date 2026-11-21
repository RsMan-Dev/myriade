//! Network ids (`Mob.id`, u32) of the entities a player follows. The id is the sparse-set component `NetId`: an entity that is not
//! followed has no byte and no entry. Its hooks keep the table `NetIds` (id to entity), so the server finds an entity by id; the
//! reverse read is the component itself. The server gives the id from its own counter.
use crate::bridge::view::Modified;
use bevy_ecs::{entity::Entity, lifecycle::HookContext, prelude::*, world::DeferredWorld};
use std::collections::HashMap;

#[derive(Resource, Default)]
pub struct NetIds {
    by_id: HashMap<u32, Entity>,
}

impl NetIds {
    /// The entity that holds the network id `id`.
    pub fn entity(&self, id: u32) -> Option<Entity> {
        self.by_id.get(&id).copied()
    }
}

/// The network ids of the followed entities removed (despawned or unbound) since the last `EcsWorld::drain_removed`.
#[derive(Resource, Default)]
pub struct Removed(pub Vec<u32>);

/// The network id of `e`, when it is followed.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet", on_add = added, on_remove = removed)]
pub struct NetId(pub u32);

/// The network id of `e`, read from its component.
pub fn id_of(world: &World, e: Entity) -> Option<u32> {
    world.get::<NetId>(e).map(|n| n.0)
}

fn added(mut w: DeferredWorld, ctx: HookContext) {
    let id = w.get::<NetId>(ctx.entity).map_or(0, |n| n.0);
    w.resource_mut::<NetIds>().by_id.insert(id, ctx.entity);
    w.resource_mut::<Modified>().push(ctx.entity);
}

fn removed(mut w: DeferredWorld, ctx: HookContext) {
    let id = w.get::<NetId>(ctx.entity).map_or(0, |n| n.0);
    let held = {
        let mut t = w.resource_mut::<NetIds>();
        let held = t.by_id.get(&id) == Some(&ctx.entity);
        if held {
            t.by_id.remove(&id);
        }
        held
    };
    if held {
        w.resource_mut::<Removed>().0.push(id);
    }
}

/// Gives the existing entity `e` the network id `id`. False, and nothing changes, when another entity holds `id` or `e` has one.
pub fn bind(world: &mut World, e: Entity, id: u32) -> bool {
    world.init_resource::<NetIds>();
    world.init_resource::<Modified>();
    world.init_resource::<Removed>();
    if world.resource::<NetIds>().by_id.contains_key(&id) || world.get::<NetId>(e).is_some() {
        return false;
    }
    world.entity_mut(e).insert(NetId(id));
    true
}

/// Takes `e` out of the table: its id is free again, and the component goes with it.
pub fn unbind(world: &mut World, e: Entity) {
    world.entity_mut(e).remove::<NetId>();
}
