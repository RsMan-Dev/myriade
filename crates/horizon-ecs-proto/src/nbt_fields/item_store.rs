//! The key of a carried inventory (`Piglin`, `Inventory`): one compound per stack with its `Slot`; the empty slots are not written.
use crate::item_store::ItemStore;
use crate::nbt::{owner, Field};
use bevy_ecs::prelude::*;
use horizon_core::nbt::{Compound, Tag};
use horizon_items::item_nbt::{item_from_nbt, item_to_nbt};
use std::borrow::Cow;

/// The keys of a carried inventory.
pub const FIELDS: &[Field] = &[Field { key: Cow::Borrowed("Inventory"), write: write_items, read: read_items, accepts: list, owner: Some(owner::<ItemStore>) }];

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
