//! The items of Horizon by name: every item of the registry (`Items`, in the registry's network order) gets an id, 0 meaning none or unknown
//! (`item_id("minecraft:saddle")`), with its tags (`has_tag`), the slot it is worn in and the armor it gives (`worn_in`, `armor_of`: read from its
//! components, not listed here). Installed once, with the level (`install`). A module that cares about a list of items holds an `ItemSet`
//! (names and `#tags` resolved once at creation), never a bit per id. Nothing here is about one species.
use horizon_client_lib::items::Items;
use std::collections::HashMap;
use std::sync::OnceLock;

pub use crate::goals::Slot;
struct Table {
    names: Vec<std::sync::Arc<str>>,
    ids: HashMap<std::sync::Arc<str>, u32>,
    tags: HashMap<String, Vec<u32>>,
    slot: Vec<Option<Slot>>,
    armor: Vec<(f64, f64)>,
    food: Vec<bool>,
    attack: Vec<f64>,
}
static TABLE: OnceLock<Table> = OnceLock::new();

/// Reads the registry of the level (a second call changes nothing).
pub fn install(items: &Items) {
    TABLE.get_or_init(|| {
        let names: Vec<std::sync::Arc<str>> = if items.registry_order.is_empty() { items.order.clone() } else { items.registry_order.clone() };
        let ids: HashMap<_, _> = names.iter().enumerate().map(|(i, n)| (n.clone(), i as u32 + 1)).collect();
        let mut slot = vec![None; names.len() + 1];
        let mut armor = vec![(0.0, 0.0); names.len() + 1];
        let mut food = vec![false; names.len() + 1];
        let mut attack = vec![0.0; names.len() + 1];
        for (i, n) in names.iter().enumerate() {
            let Some(info) = items.get(n) else { continue };
            slot[i + 1] = match info.equip_slot.as_deref() {
                Some("head") => Some(Slot::Head),
                Some("chest") => Some(Slot::Chest),
                Some("legs") => Some(Slot::Legs),
                Some("feet") => Some(Slot::Feet),
                Some("body") => Some(Slot::Body),
                Some("saddle") => Some(Slot::Saddle),
                Some("offhand") => Some(Slot::Off),
                _ => None,
            };
            for m in &info.modifiers {
                match m.attribute.trim_start_matches("minecraft:") {
                    "armor" if m.operation == "add_value" => armor[i + 1].0 += m.amount,
                    "armor_toughness" if m.operation == "add_value" => armor[i + 1].1 += m.amount,
                    _ => {}
                }
            }
            food[i + 1] = info.food.is_some();
            attack[i + 1] = f64::from(info.attack_damage);
        }
        let mut tags: HashMap<String, Vec<u32>> = HashMap::new();
        for (t, members) in &items.item_tags {
            tags.insert(t.trim_start_matches('#').to_owned(), members.iter().filter_map(|m| ids.get(m.as_str()).copied()).collect());
        }
        Table { names, ids, tags, slot, armor, food, attack }
    });
}
pub fn installed() -> bool {
    TABLE.get().is_some()
}
/// The id of an item by its name (`minecraft:wheat`); 0 when there is no registry or no such item.
pub fn item_id(name: &str) -> u32 {
    let Some(t) = TABLE.get() else { return 0 };
    t.ids.get(name).copied().unwrap_or(0)
}
pub fn item_name(id: u32) -> Option<&'static str> {
    TABLE.get()?.names.get((id as usize).checked_sub(1)?).map(|n| &**n)
}
/// Whether the item is in the tag (`minecraft:horse_food`, with or without the `#`).
pub fn has_tag(tag: &str, id: u32) -> bool {
    TABLE.get().and_then(|t| t.tags.get(tag.trim_start_matches('#'))).is_some_and(|v| v.contains(&id))
}
/// The slot an item is worn in, if it is equippable.
pub fn worn_in(id: u32) -> Option<Slot> {
    TABLE.get()?.slot.get(id as usize).copied().flatten()
}
/// The armor and the toughness an item gives while worn (`attribute_modifiers`, `add_value`).
pub fn armor_of(id: u32) -> (f64, f64) {
    TABLE.get().and_then(|t| t.armor.get(id as usize)).copied().unwrap_or((0.0, 0.0))
}
pub fn attack_damage_of(id: u32) -> f64 {
    TABLE.get().and_then(|t| t.attack.get(id as usize)).copied().unwrap_or(0.0)
}
pub fn is_food(id: u32) -> bool {
    TABLE.get().and_then(|t| t.food.get(id as usize)).copied().unwrap_or(false)
}
/// A list of items, by names and `#tags`, resolved once.
#[derive(Clone, Copy, Debug)]
pub struct ItemSet(&'static [u32]);
impl ItemSet {
    pub const EMPTY: Self = Self(&[]);
    pub fn of(names: &[&str]) -> Self {
        let mut v: Vec<u32> = Vec::new();
        for n in names {
            match n.strip_prefix('#') {
                Some(tag) => v.extend(TABLE.get().and_then(|t| t.tags.get(tag)).into_iter().flatten()),
                None => v.push(item_id(n)),
            }
        }
        v.retain(|&i| i != 0);
        v.sort_unstable();
        v.dedup();
        Self(Box::leak(v.into_boxed_slice()))
    }
    pub fn contains(&self, id: u32) -> bool {
        id != 0 && self.0.binary_search(&id).is_ok()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn first(&self) -> Option<u32> {
        self.0.first().copied()
    }
}
/// Whether the item is the one `name` says, or in the tag when `name` is `#tag`.
pub fn is(name: &str, id: u32) -> bool {
    id != 0 && name.strip_prefix('#').map_or_else(|| item_id(name) == id, |t| has_tag(t, id))
}
