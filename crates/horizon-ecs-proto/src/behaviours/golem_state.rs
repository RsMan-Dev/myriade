//! The copper golem's animation state (CopperGolem.COPPER_GOLEM_STATE), read by the entity-data sync. Data only; absent = idle.
use bevy_ecs::prelude::*;
use myriade_engine_lib::mobs::copper_golem as cg;

/// The current animation; removed when it returns to idle (rare state, sparse).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[component(storage = "SparseSet")]
pub struct GolemState(pub u8);

/// The state of an entity: idle without the component.
pub fn of(w: &World, e: Entity) -> u8 {
    w.get::<GolemState>(e).map_or(cg::IDLE, |s| s.0)
}

/// Sets the state; idle removes the component.
pub fn set(w: &mut World, e: Entity, state: u8) {
    let Ok(mut ent) = w.get_entity_mut(e) else {
        return;
    };
    if state == cg::IDLE {
        ent.remove::<GolemState>();
    } else {
        ent.insert(GolemState(state));
    }
}
