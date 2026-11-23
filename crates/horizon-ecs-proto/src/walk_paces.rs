//! The walk that sets a pose and a sprint by its speed (`Cat`, `Ocelot.customServerAiStep`: a move speed of 1.33 sprints, 0.6 crouches,
//! any other stands). The species' rule gives `WalkPaces`; the walk writers call `pace` when they ask for a new speed, and the path's
//! end clears it. Nothing runs per tick, and no other species holds the marker.
use crate::pose_state::{self, CROUCHING};
use crate::rules::{Rule, no_attrs, no_interact};
use crate::sprinting::Sprinting;
use bevy_ecs::prelude::*;

/// The walk of this creature paces its sprint and its crouch (`Cat`, `Ocelot`).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct WalkPaces;

/// The rule that gives a species its `WalkPaces`.
pub static PACES: Rule = Rule {
    name: "walk_paces",
    equip: |w, e, _| {
        w.entity_mut(e).insert(WalkPaces);
    },
    attrs: no_attrs,
    interact: no_interact,
};

/// The move speed that sprints (`Cat`'s and `Ocelot`'s 1.33) and the one that crouches (0.6).
const SPRINT: f64 = 1.33;
const CROUCH: f64 = 0.6;

/// Sets the sprint and the crouch of a creature with `WalkPaces` for the new walk `speed` (0 when the walk ends).
pub fn pace(w: &mut World, e: Entity, speed: f64) {
    if w.get::<WalkPaces>(e).is_none() {
        return;
    }
    let crouched = pose_state::get(w, e) == Some(CROUCHING);
    if speed == SPRINT {
        w.entity_mut(e).insert(Sprinting);
    } else {
        w.entity_mut(e).remove::<Sprinting>();
    }
    if speed == CROUCH {
        pose_state::set(w, e, Some(CROUCHING));
    } else if crouched {
        pose_state::set(w, e, None);
    }
}
