//! The trident's outputs toward the host: the blows, the thunder of Channeling, the plant, the return of a Loyalty trident and the pickup.
//! They queue in `TridentOut` as they happen (an event, never a scan) and the host drains them with `bridge::trident::drain_events`.
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use myriade_engine_lib::items::ItemStack;

#[derive(Clone, Debug, PartialEq)]
pub enum TridentEvent {
    /// The trident hit `victim` with the base blow (`base`; the flight dealt base plus the enchantment bonus), thrown by `by`.
    Hit { entity: Entity, by: Option<Who>, victim: Who, base: f32 },
    /// Channeling: a lightning bolt at `at`, caused by `cause` (the host summons it).
    Thunder { at: [f64; 3], cause: Option<Who> },
    /// The trident stuck in a block: its sound.
    Landed { entity: Entity },
    /// A Loyalty trident reached its owner and went back.
    Returned { entity: Entity, owner: Option<Who> },
    /// `by` took the trident, with its stack.
    PickedUp { entity: Entity, by: Who, stack: ItemStack },
}

/// The queue of the trident world's events, emptied by `drain_events` at each step.
#[derive(Resource, Default)]
pub struct TridentOut(pub Vec<TridentEvent>);
