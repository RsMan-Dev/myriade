//! A creature that any use by a player keeps for good (`setPersistenceRequired` in the entity's `interact`, taken or not):
//! the rule `keep_on_use` (no params) gives it the marker, and `goals::interact` reads the marker before any module answers.
use crate::rules::{no_attrs, no_interact, Rule};
use bevy_ecs::prelude::*;

#[derive(Component, Clone, Copy, Debug)]
#[cfg_attr(feature = "sparse", component(storage = "SparseSet"))]
pub struct KeepOnUse;

pub static KEEP_ON_USE: Rule = Rule { name: "keep_on_use", equip: |w, e, _| { w.entity_mut(e).insert(KeepOnUse); }, attrs: no_attrs, interact: no_interact };

/// Called by the interaction step for every use of `e`.
pub fn on_use(w: &mut World, e: Entity) {
    if w.get::<KeepOnUse>(e).is_some() {
        crate::despawn::keep(w, e);
    }
}
