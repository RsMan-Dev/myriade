//! The chest of a mount that carries one (a donkey, a mule, a llama with a chest): its slots, row by row, on one sparse component. The
//! columns are the chest's width (`AbstractChestedHorse.getInventoryColumns`: 5 with a chest; a llama's is its `Strength`), given when
//! the chest goes on. Written by the host's events only (the chest put on or taken off, the menu closed), never by a tick. Nothing here
//! names a species.
use crate::behaviours::forget_target::LlamaStrength;
use crate::goals::Slot;
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;

/// The slots of a chest, `columns * 3` of them (`None` empty). Only a mount that carries a chest has it.
#[derive(Component, Clone, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct MountInventory {
    pub columns: u8,
    pub items: Vec<Option<ItemStack>>,
}

/// The columns a chest goes on with: a llama's strength (`Llama.getInventoryColumns`, the strength itself), else `configured` (5 for a
/// donkey or a mule, `AbstractChestedHorse.getInventoryColumns`). A llama of strength 0 has no column.
pub fn chest_columns(world: &World, e: Entity, configured: u8) -> u8 {
    world.get::<LlamaStrength>(e).map_or(configured, |s| u8::try_from(s.0).unwrap_or(0))
}

/// Puts a chest of `columns` columns on `e` (the host gives `chest_columns`): the slots it had stay, the new ones are empty. Nothing when `e`
/// does not exist.
pub fn chest_on(world: &mut World, e: Entity, columns: u8) {
    if world.get_entity(e).is_err() {
        return;
    }
    let mut items = world.get::<MountInventory>(e).map_or_else(Vec::new, |m| m.items.clone());
    items.resize(usize::from(columns) * 3, None);
    world.entity_mut(e).insert(MountInventory { columns, items });
}

/// Writes the slots of the chest of `e`, cut or padded to its `columns * 3`. False without a chest on `e`.
pub fn set_items(world: &mut World, e: Entity, items: Vec<Option<ItemStack>>) -> bool {
    let Some(mut m) = world.get_mut::<MountInventory>(e) else { return false };
    let mut items = items;
    items.resize(usize::from(m.columns) * 3, None);
    m.items = items;
    true
}

/// Takes the chest off `e`: its component goes and its items are returned, for the caller to drop (none without a chest).
pub fn chest_off(world: &mut World, e: Entity) -> Vec<Option<ItemStack>> {
    let items = world.get_mut::<MountInventory>(e).map_or_else(Vec::new, |mut m| std::mem::take(&mut m.items));
    if let Ok(mut q) = world.get_entity_mut(e) {
        q.remove::<MountInventory>();
    }
    items
}

/// What a horse leaves at death (`AbstractHorse.dropEquipment`, then `AbstractChestedHorse`): its saddle and armour, the chest's items,
/// then the chest itself. Each piece is taken off its slot, the whole stack kept. Nothing for a creature that is not a horse.
pub fn death_stacks(world: &mut World, e: Entity) -> Vec<ItemStack> {
    if !world.get::<crate::mount::Mount>(e).is_some_and(|m| m.max_temper > 0) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (slot, goal) in [(7, Slot::Saddle), (6, Slot::Body)] {
        if let Some(stack) = crate::equip_stack::get(world, e, slot) {
            crate::equip_stack::set(world, e, slot, None);
            crate::equipment::set_slot(world, e, goal, 0);
            out.push(stack);
        }
    }
    if world.get::<MountInventory>(e).is_some() {
        out.extend(chest_off(world, e).into_iter().flatten());
        out.push(ItemStack::new("minecraft:chest", 1));
    }
    out
}
