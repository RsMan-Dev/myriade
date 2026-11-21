//! The outputs of the item use toward the host: a use that starts, ends or is released, and a totem that pops. Not in `host_out.rs`: at
//! fusion this enum becomes `HostEvent::Using(UsingEvent)`, and the queue is read by `EcsWorld::using_events`.
use bevy_ecs::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UsingEvent {
    /// `entity` started a use with its off hand or its main hand.
    Started { entity: Entity, off_hand: bool },
    /// The use of `entity` ended by its deadline or by `stop_using`.
    Stopped { entity: Entity },
    /// `entity` released its use with `charged` ticks left.
    Released { entity: Entity, charged: i32 },
    /// A totem of `entity` was used up in its off hand or its main hand.
    TotemPopped { entity: Entity, off_hand: bool },
}

/// The same events as the host reads them: the entity by its network id, bound on the way out (`EcsWorld::using_events`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UsingNetEvent {
    Started { id: u32, off_hand: bool },
    Stopped { id: u32 },
    Released { id: u32, charged: i32 },
    TotemPopped { id: u32, off_hand: bool },
}

/// The events written since the host last read them, in order: empty at rest.
#[derive(Resource, Default, Debug)]
pub struct UsingOut(pub Vec<UsingEvent>);
