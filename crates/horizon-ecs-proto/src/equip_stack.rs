//! The whole stack each equipment slot of an entity holds (enchantments, wear and components of the host's `ItemStack`), on one sparse
//! component. An entity that wears nothing has none. Written by event only (the host's write, the loot, a swap, a hit): `set` and `damage`
//! queue an `EquipmentEvent` in `EquipmentOut`; nothing is compared per tick. Nothing here names a species.
use crate::equipment_items::durability_of;
use crate::host_out_equipment::{EquipmentEvent, EquipmentOut};
use crate::items::item_id;
use bevy_ecs::prelude::*;
use myriade_engine_lib::items::ItemStack;

/// The slots, in the order of `EquipmentSlot`: main, off, head, chest, legs, feet, body, saddle.
pub const SLOTS: usize = 8;
/// `Equippable.Builder`'s default equip sound, `SoundEvents.ARMOR_EQUIP_GENERIC`.
pub const EQUIP_GENERIC: &str = "minecraft:item.armor.equip_generic";

#[derive(Component, Clone, Debug, Default, PartialEq)]
#[component(storage = "SparseSet")]
pub struct Equipment {
    pub slots: [Option<ItemStack>; SLOTS],
}

/// The stack of `slot` of `e`; None when the slot is empty, the entity wears nothing or `slot` is out of range.
pub fn get(world: &World, e: Entity, slot: usize) -> Option<ItemStack> {
    world.get::<Equipment>(e)?.slots.get(slot)?.clone()
}

/// Writes `stack` in `slot` of `e` (None empties it). The component comes with the first piece and goes with the last one. False when `e`
/// does not exist or `slot` is out of range; writing the stack already there is accepted and queues nothing.
pub fn set(world: &mut World, e: Entity, slot: usize, stack: Option<ItemStack>) -> bool {
    if slot >= SLOTS || world.get_entity(e).is_err() {
        return false;
    }
    let stack = stack.filter(|s| s.count > 0);
    let from = get(world, e, slot);
    if from == stack {
        return true;
    }
    if world.get::<Equipment>(e).is_none() {
        world.entity_mut(e).insert(Equipment::default());
    }
    let empty = {
        let mut eq = world.get_mut::<Equipment>(e).expect("inserted above");
        eq.slots[slot] = stack.clone();
        eq.slots.iter().all(Option::is_none)
    };
    if empty {
        world.entity_mut(e).remove::<Equipment>();
    }
    crate::equip_attrs::changed(world, e, slot, stack.as_ref());
    push(world, EquipmentEvent::Changed { entity: e, slot, from, to: stack });
    true
}

/// `ItemStack.hurtAndBreak` without Unbreaking: `amount` points of wear on `slot` of `e`. A piece whose wear reaches its durability leaves
/// the slot, with a `Broke` after its `Changed`. False when the slot holds nothing that wears.
pub fn damage(world: &mut World, e: Entity, slot: usize, amount: i32) -> bool {
    let Some(mut stack) = get(world, e, slot) else {
        return false;
    };
    let max = durability_of(item_id(&stack.item)).max(0) as u32;
    if max == 0 {
        return false;
    }
    stack.damage = stack.damage.saturating_add(amount.max(0) as u32);
    let broke = stack.damage >= max;
    set(world, e, slot, (!broke).then_some(stack));
    if broke {
        push(world, EquipmentEvent::Broke { entity: e, slot });
    }
    true
}

/// The `Changed` events since the last call, in order; the `Broke` ones stay queued for `take_broken`.
pub fn take_changed(world: &mut World) -> Vec<(Entity, usize, Option<ItemStack>, Option<ItemStack>)> {
    let queue = &mut world.get_resource_or_insert_with(EquipmentOut::default).0;
    let (changed, kept): (Vec<_>, Vec<_>) = std::mem::take(queue).into_iter().partition(|ev| matches!(ev, EquipmentEvent::Changed { .. }));
    *queue = kept;
    changed
        .into_iter()
        .filter_map(|ev| match ev {
            EquipmentEvent::Changed { entity, slot, from, to } => Some((entity, slot, from, to)),
            EquipmentEvent::Broke { .. } => None,
        })
        .collect()
}

/// The `Broke` events since the last call, in order.
pub fn take_broken(world: &mut World) -> Vec<(Entity, usize)> {
    let queue = &mut world.get_resource_or_insert_with(EquipmentOut::default).0;
    let (broke, kept): (Vec<_>, Vec<_>) = std::mem::take(queue).into_iter().partition(|ev| matches!(ev, EquipmentEvent::Broke { .. }));
    *queue = kept;
    broke
        .into_iter()
        .filter_map(|ev| match ev {
            EquipmentEvent::Broke { entity, slot } => Some((entity, slot)),
            EquipmentEvent::Changed { .. } => None,
        })
        .collect()
}

fn push(world: &mut World, event: EquipmentEvent) {
    world.get_resource_or_insert_with(EquipmentOut::default).0.push(event);
}
