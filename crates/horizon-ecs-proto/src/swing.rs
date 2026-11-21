//! `LivingEntity.swing` (the arm swing of a creature): `SwingState.startIfAble` and the packet the clients animate from.
//! The swing's state is kept only while it runs (a sparse component removed by a deadline); the animation itself is the clients'.
//! DOUTE: the duration is the default 6; haste, conduit power and mining fatigue (`getModifiedSwingDuration`) are not represented.
use crate::cow::*;
use crate::goals::Events;
use bevy_ecs::prelude::*;

/// The default swing duration, in ticks (`SwingAnimation` of an arm: `LivingEntity.getCurrentSwingDuration`).
pub const DURATION: u64 = 6;

/// A swing that runs: the own time it started, its length and the hand.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Swinging {
    pub start: u64,
    pub duration: u64,
    pub off_hand: bool,
}

/// Whether the creature is swinging now.
pub fn is_swinging(w: &World, e: Entity) -> bool {
    w.get::<Swinging>(e).is_some_and(|s| now_of(w, e) < s.start + s.duration)
}

/// `LivingEntity.swing(hand)`: starts a swing unless one is running and still in its first half (`startIfAble`); the host is told
/// when it started. Returns whether it started.
pub fn swing(w: &mut World, e: Entity, off_hand: bool) -> bool {
    let now = now_of(w, e);
    if let Some(s) = w.get::<Swinging>(e) {
        if now < s.start + s.duration && now - s.start < s.duration / 2 {
            return false;
        }
    }
    w.entity_mut(e).insert(Swinging { start: now, duration: DURATION, off_hand });
    w.resource_mut::<Events>().host.swing(e, off_hand);
    after(w, e, DURATION as i32, over);
    true
}

/// The deadline of a swing: its state is dropped if no later swing replaced it.
fn over(w: &mut World, e: Entity) {
    if w.get_entity(e).is_ok() && !is_swinging(w, e) {
        w.entity_mut(e).remove::<Swinging>();
    }
}
