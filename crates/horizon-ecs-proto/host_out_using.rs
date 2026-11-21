//! The output of the item use toward the host: a totem that pops (its animation). A use that starts, ends or is released has no
//! event: the host's own calls (`start_mob_using_item_ecs`, `release_mob_using_item_ecs`) did their effect when they asked.
use bevy_ecs::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UsingEvent {
    /// A totem of `entity` was used up in its off hand or its main hand.
    TotemPopped { entity: Entity, off_hand: bool },
}

/// The events written since the host last read them, in order: empty at rest.
#[derive(Resource, Default, Debug)]
pub struct UsingOut(pub Vec<UsingEvent>);
