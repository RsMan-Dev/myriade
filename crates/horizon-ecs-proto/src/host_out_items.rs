//! The outputs of the items and the orbs toward the host: an end of life held for the host to decide (`Expiring`, only when the host
//! listens, `ExpiryPolicy`). Not in `host_out.rs`: at fusion this enum becomes `HostEvent::Item(ItemEvent)`; `ItemEvents` is the queue
//! that `EcsWorld::drain_item_events` empties.
use bevy_ecs::{entity::Entity, prelude::Resource};
use horizon_client_lib::items::ItemStack;

#[derive(Clone, Debug, PartialEq)]
pub enum ItemEvent {
    /// A stack reached its end of life and is held (`item::Expiring`) until the host extends it or removes it.
    Expiring { entity: Entity, stack: ItemStack, at: [f64; 3] },
    /// An orb of `value` reached its end of life and is held (`item::Expiring`) until the host extends it or removes it.
    /// `ItemEntity.hurtServer` took damage: the host sends `gameEvent(ENTITY_DAMAGE, source.getEntity())` at `at`, before a death.
    Hurt { at: [f64; 3], by: Option<crate::targeting::Who> },
    OrbExpiring { entity: Entity, value: i32, at: [f64; 3] },
}

#[derive(Resource, Default)]
pub struct ItemEvents(pub Vec<ItemEvent>);
