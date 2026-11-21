//! The keys of a mount's chest (`ChestedHorse`, `Items`): the chest is on and its slots, each with its `Slot` index. Only a mount with a chest
//! writes them. A llama's `Strength` is the creatures' key (`creatures.rs`), not this one.
use crate::mount_inventory::{chest_columns, chest_on, set_items, MountInventory};
use crate::nbt::{owner, Field};
use bevy_ecs::prelude::*;
use horizon_core::nbt::{Compound, Tag};
use horizon_items::item_nbt::{item_from_nbt, item_to_nbt};
use std::borrow::Cow;

/// The columns of a donkey's or a mule's chest (`AbstractChestedHorse.getInventoryColumns`: 5 with a chest).
const CHEST_COLUMNS: u8 = 5;

/// The keys of a mount's chest, in the jar's order of writing.
pub const FIELDS: &[Field] = &[
    Field { key: Cow::Borrowed("ChestedHorse"), write: write_chested, read: read_chested, accepts: byte, owner: Some(owner::<MountInventory>) },
    Field { key: Cow::Borrowed("Items"), write: write_items, read: read_items, accepts: list, owner: Some(owner::<MountInventory>) },
];

fn byte(t: &Tag) -> bool {
    matches!(t, Tag::Byte(_))
}
fn list(t: &Tag) -> bool {
    matches!(t, Tag::List(_))
}

fn write_chested(w: &World, e: Entity, c: &mut Compound) {
    if w.get::<MountInventory>(e).is_some() {
        c.insert("ChestedHorse".into(), Tag::Byte(1));
    }
}

fn read_chested(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    if *v != 0 && w.get::<MountInventory>(e).is_none() {
        let columns = chest_columns(w, e, CHEST_COLUMNS);
        chest_on(w, e, columns);
    }
    true
}

/// `Items`: one compound per stack, its item's keys and its `Slot`; the empty slots are not written.
fn write_items(w: &World, e: Entity, c: &mut Compound) {
    let Some(m) = w.get::<MountInventory>(e) else { return };
    let items = m
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
    c.insert("Items".into(), Tag::List(items));
}

fn read_items(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::List(list) = t else { return false };
    let Some(len) = w.get::<MountInventory>(e).map(|m| m.items.len()) else { return true };
    let mut items = vec![None; len];
    for entry in list {
        let (Some(Tag::Byte(slot)), Some(stack)) = (entry.get("Slot"), item_from_nbt(entry)) else { continue };
        if let Some(cell) = usize::try_from(*slot).ok().and_then(|i| items.get_mut(i)) {
            *cell = Some(stack);
        }
    }
    set_items(w, e, items);
    true
}
