//! The rule `FALL_BY_HEALTH`: a creature with a target may fall `3 + (health - 1)` blocks on its path (`Creeper`'s max fall, as
//! its health). The species fiche gives the rule; the marker is a sparse component, so the others pay nothing.
use crate::rules::{no_attrs, no_interact, Rule};
use bevy_ecs::prelude::*;

/// The marker of the rule: the creature's path allows the fall its health gives while it has a target.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct FallByHealth;

pub static FALL_BY_HEALTH: Rule = Rule { name: "fall_by_health", equip, attrs: no_attrs, interact: no_interact };

fn equip(w: &mut World, e: Entity, _: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(FallByHealth);
}

/// The max fall of a path (`pathfind`): a creature with the rule and a target falls `3 + (health - 1)`; otherwise the comfortable fall.
pub fn path_max_fall(health: Option<f32>, has_target: bool) -> i32 {
    match health {
        Some(h) if has_target => 3 + (h as i32 - 1),
        _ => crate::cow::comfortable_fall_distance(0.0),
    }
}
