//! The break of a piece, as the entity event vanilla sends (`LivingEntity.entityEventForEquipmentBreak`, the table of Horizon's `item_break.rs`):
//! 47 main hand, 48 off hand, 49 head, 50 chest, 51 legs, 52 feet. `HostOut` carries no entity event, so the breaks wait in `BreakEvents`,
//! which the host drains each tick next to `HostOut`.
use crate::goals::Slot;
use bevy_ecs::prelude::*;

/// The entity event of a slot's break; `None` for a slot with no break event here (body and saddle are not made by the equipment).
pub fn event_of(slot: Slot) -> Option<u8> {
    match slot {
        Slot::Main => Some(47),
        Slot::Off => Some(48),
        Slot::Head => Some(49),
        Slot::Chest => Some(50),
        Slot::Legs => Some(51),
        Slot::Feet => Some(52),
        _ => None,
    }
}

/// The breaks of this tick: (entity, entity event), in the order they happened.
#[derive(Resource, Default, Debug)]
pub struct BreakEvents(pub Vec<(Entity, u8)>);

/// Records the break of `slot` on `e`; nothing for a slot without an event.
pub fn broke(w: &mut World, e: Entity, slot: Slot) {
    if let Some(event) = event_of(slot) {
        w.get_resource_or_insert_with(BreakEvents::default).0.push((e, event));
    }
}
