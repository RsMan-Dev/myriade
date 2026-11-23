//! The outputs of the equipment toward the host: a slot whose stack changed, and a piece that broke. Not in `host_out.rs`: at fusion this
//! enum becomes `HostEvent::Equipment(EquipmentEvent)`, and the queue is read by `EcsWorld::equipment_changes` and `equipment_broken`.
use bevy_ecs::prelude::*;
use myriade_engine_lib::items::ItemStack;

#[derive(Clone, Debug, PartialEq)]
pub enum EquipmentEvent {
    /// The stack of `slot` of `entity` went from `from` to `to` (None: empty).
    Changed { entity: Entity, slot: usize, from: Option<ItemStack>, to: Option<ItemStack> },
    /// The piece of `slot` of `entity` wore out and left its slot (after its `Changed`).
    Broke { entity: Entity, slot: usize },
}

/// The events written since the host last read them, in order: empty at rest.
#[derive(Resource, Default, Debug)]
pub struct EquipmentOut(pub Vec<EquipmentEvent>);
