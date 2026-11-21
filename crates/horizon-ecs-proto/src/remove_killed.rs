//! `Entity.remove(KILLED)` after a lethal hit (`ShootTongue.eatEntity`: `doHurtTarget`, then `remove(KILLED)` when the prey is dead): the death
//! runs whole (said to mods, credit, loot, experience) but the client plays no death animation, the body is gone. `mark` is set on the
//! target just before the hit; a hit that does not kill leaves it harmless, for it lapses after `WINDOW` ticks of the level.
use crate::cow::Clock;
use bevy_ecs::prelude::*;

/// The ticks the mark lasts: the hit is applied by the damage step of this tick or the next.
pub const WINDOW: u64 = 2;

#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct RemoveKilled {
    until: u64,
}

/// `e` is about to be hit by a removal that is a death without animation.
pub fn mark(w: &mut World, e: Entity) {
    let until = w.resource::<Clock>().0 + WINDOW;
    if let Ok(mut m) = w.get_entity_mut(e) {
        m.insert(RemoveKilled { until });
    }
}

/// True when the death of `e` now is one of those (no death animation).
pub fn silent(w: &World, e: Entity) -> bool {
    w.get::<RemoveKilled>(e).is_some_and(|r| w.resource::<Clock>().0 <= r.until)
}
