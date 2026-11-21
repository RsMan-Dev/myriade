//! The uuid of an entity the host names (`EntityUuid`, a sparse-set component: absent by default, laid only when the host writes it),
//! and the table uuid to entity that its hooks keep (`Uuids`), as `ids.rs` keeps the network ids. The uuid is held as one `u128`
//! and read as Java's four words (most significant first).
use bevy_ecs::{entity::Entity, lifecycle::HookContext, prelude::*, world::DeferredWorld};
use std::collections::HashMap;

#[derive(Resource, Default)]
pub struct Uuids {
    by_uuid: HashMap<u128, Entity>,
}

/// The uuid of `e`, when the host has written one.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet", on_add = added, on_remove = removed)]
pub struct EntityUuid(pub u128);

fn added(mut w: DeferredWorld, ctx: HookContext) {
    let u = w.get::<EntityUuid>(ctx.entity).map_or(0, |u| u.0);
    w.resource_mut::<Uuids>().by_uuid.insert(u, ctx.entity);
}

fn removed(mut w: DeferredWorld, ctx: HookContext) {
    let u = w.get::<EntityUuid>(ctx.entity).map_or(0, |u| u.0);
    let mut t = w.resource_mut::<Uuids>();
    if t.by_uuid.get(&u) == Some(&ctx.entity) {
        t.by_uuid.remove(&u);
    }
}

/// The four words of a uuid as the save writes them, to one `u128`.
pub(crate) fn from_words(w: [i32; 4]) -> u128 {
    w.iter().fold(0u128, |acc, &x| (acc << 32) | u128::from(x as u32))
}

/// The four words of a uuid, most significant first (`UUIDUtil.uuidToIntArray`).
pub fn to_words(v: u128) -> [i32; 4] {
    [(v >> 96) as u32 as i32, (v >> 64) as u32 as i32, (v >> 32) as u32 as i32, v as u32 as i32]
}

/// Gives `e` the uuid `uuid`, replacing its own. The table follows through the hooks.
pub fn set_uuid(world: &mut World, e: Entity, uuid: [i32; 4]) {
    world.init_resource::<Uuids>();
    world.entity_mut(e).remove::<EntityUuid>().insert(EntityUuid(from_words(uuid)));
    crate::player_uuids::resolve_entity(world, from_words(uuid), e);
}

/// The entity that holds `uuid`; None when no live entity has it.
pub fn entity_by_uuid(world: &World, uuid: [i32; 4]) -> Option<Entity> {
    world.get_resource::<Uuids>()?.by_uuid.get(&from_words(uuid)).copied()
}

impl crate::bridge::host::EcsWorld {
    /// Gives `e` the uuid `uuid` (see `set_uuid`).
    pub fn set_uuid(&mut self, e: Entity, uuid: [i32; 4]) {
        set_uuid(self.world_mut(), e, uuid);
    }

    /// The entity of `uuid`, None once it is despawned.
    pub fn entity_by_uuid(&self, uuid: [i32; 4]) -> Option<Entity> {
        entity_by_uuid(self.world(), uuid)
    }
}
