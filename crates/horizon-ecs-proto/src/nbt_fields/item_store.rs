//! The key of a carried inventory (`Piglin`, `Inventory`): one compound per stack with its `Slot`; the empty slots are not written.
//! A villager's `Inventory` is its food stock.
use crate::behaviours::food_inventory::{Stock, SLOTS};
use crate::behaviours::trade_offers::Merchant;
use crate::item_store::ItemStore;
use crate::nbt::{owner, Field};
use bevy_ecs::prelude::*;
use horizon_core::nbt::{Compound, Tag};
use horizon_items::item_nbt::{item_from_nbt, item_to_nbt};
use horizon_items::items::ItemStack;
use std::borrow::Cow;
use std::sync::Arc;

/// The keys of a carried inventory, and of a villager's food.
pub const FIELDS: &[Field] = &[
    Field { key: Cow::Borrowed("Inventory"), write: write_items, read: read_items, accepts: list, owner: Some(owner::<ItemStore>) },
    Field { key: Cow::Borrowed("Inventory"), write: write_stock, read: read_stock, accepts: list, owner: Some(owner::<Merchant>) },
];

fn list(t: &Tag) -> bool {
    matches!(t, Tag::List(_))
}

fn write_items(w: &World, e: Entity, c: &mut Compound) {
    let Some(s) = w.get::<ItemStore>(e) else { return };
    let items = s
        .items
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            let mut t = item_to_nbt(s.as_ref()?);
            if let Tag::Compound(entry) = &mut t {
                entry.insert("Slot".into(), Tag::Byte(i as i8));
            }
            Some(t)
        })
        .collect();
    c.insert("Inventory".into(), Tag::List(items));
}

fn read_items(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::List(list) = t else { return false };
    let Some(mut s) = w.get_mut::<ItemStore>(e) else { return true };
    for slot in s.items.iter_mut() {
        *slot = None;
    }
    for entry in list {
        let (Some(Tag::Byte(slot)), Some(stack)) = (entry.get("Slot"), item_from_nbt(entry)) else { continue };
        if let Some(cell) = usize::try_from(*slot).ok().and_then(|i| s.items.get_mut(i)) {
            *cell = Some(stack);
        }
    }
    true
}

/// The villager's stock as `Inventory`: one stack per filled slot (`SimpleContainer.storeAsItemList`: no `Slot`, the stacks in order).
/// An empty stock is saved as an empty list too (`AbstractVillager` writes `Inventory` through `InventoryCarrier`, wandering traders included).
fn write_stock(w: &World, e: Entity, c: &mut Compound) {
    let items = w.get::<Stock>(e).map_or_else(Vec::new, |s| {
        s.0.iter()
            .map(|(name, n)| {
                let stack = ItemStack { item: Arc::from(name.as_str()), count: u32::from(*n), damage: 0, extra: None };
                item_to_nbt(&stack)
            })
            .collect()
    });
    c.insert("Inventory".into(), Tag::List(items));
}

/// Reads the stacks in the order of their `Slot`, the first `SLOTS` ones; an empty list removes the stock.
fn read_stock(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::List(list) = t else { return false };
    let mut slots: Vec<(i8, (String, u8))> = list
        .iter()
        .enumerate()
        .filter_map(|(i, entry)| {
            let stack = item_from_nbt(entry)?;
            let slot = match entry.get("Slot") {
                Some(Tag::Byte(slot)) => *slot,
                _ => i as i8,
            };
            Some((slot, (stack.item.to_string(), stack.count.min(255) as u8)))
        })
        .collect();
    slots.sort_by_key(|s| s.0);
    let items: Vec<(String, u8)> = slots.into_iter().take(SLOTS).map(|s| s.1).collect();
    if items.is_empty() {
        w.entity_mut(e).remove::<Stock>();
    } else {
        w.entity_mut(e).insert(Stock(items));
    }
    true
}
