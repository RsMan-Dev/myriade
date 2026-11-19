//! Block states with properties, for every module: the world's blocks are read and written with their properties (`age` of the
//! berries, the half of a door, the block an enderman carries), through the real registry (`BlockRegistry`). Writing is an event
//! (`StateChanges`, applied by `block_changes` with the same light, hazard, path and wake-up effects as a named block).
use crate::cow::{Level, StateChanges};
use bevy_ecs::prelude::*;
use horizon_client_lib::mobs::MobWorld;
use horizon_world::registry::BlockStateId;

/// The state of the block at `p` (its id).
pub fn state_id(mw: &MobWorld, p: [i32; 3]) -> BlockStateId {
    mw.world.block(p[0], p[1], p[2])
}
/// The value of a property of the block at `p` (`age`, `half`, `facing`...), or `None` when the block has no such property.
pub fn property(mw: &MobWorld, p: [i32; 3], key: &str) -> Option<String> {
    mw.registry.state(state_id(mw, p))?.properties.get(key).cloned()
}
/// The integer value of a property (`age` ...).
pub fn int_property(mw: &MobWorld, p: [i32; 3], key: &str) -> Option<i32> {
    property(mw, p, key)?.parse().ok()
}
/// The id of the block `name` with these properties over its default state; `None` for an unknown block, property or value.
pub fn resolve(mw: &MobWorld, name: &str, props: &[(&str, &str)]) -> Option<BlockStateId> {
    let default = mw.registry.default_state(name)?;
    if props.is_empty() {
        return Some(default);
    }
    let mut state = mw.registry.state(default)?.clone();
    for (k, v) in props {
        if !state.properties.contains_key(*k) {
            return None;
        }
        state.properties.insert((*k).into(), (*v).into());
    }
    mw.registry.id(&state)
}
/// Asks for the block `name` with `props` at `p`, taken at the next step (a module writes the world only through the frame).
pub fn set_state(w: &mut World, p: [i32; 3], name: &'static str, props: &'static [(&'static str, &'static str)]) {
    w.resource_mut::<StateChanges>().0.push((p[0], p[1], p[2], name, props));
}
/// The world of the blocks of the frame, for the modules that read blocks (`blocks::property(world_of(w), ..)`).
pub fn world_of(w: &World) -> &'static MobWorld<'static> {
    w.resource::<Level>().mw.0
}
/// Asks for the block state `id` at `p` (made with `resolve` from names and properties the module owns: no `&'static` needed).
pub fn set_state_id(w: &mut World, p: [i32; 3], id: BlockStateId) {
    w.resource_mut::<StateChanges>().1.push((p[0], p[1], p[2], id));
}
