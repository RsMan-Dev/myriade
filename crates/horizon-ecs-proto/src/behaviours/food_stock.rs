//! Réserve de nourriture (`Villager.foodLevel` + points de l'inventaire) : ce qui permet de se reproduire (`canBreed`, 12) et ce
//! qui est digéré. Les points de l'inventaire viennent de `food_inventory::points`, lus dans les données.
use crate::behaviours::food_inventory::{self as inv, Stock};
use crate::nbt::{accept, owner, Field};
use bevy_ecs::prelude::*;
use crate::cow::Level;
use horizon_client_lib::items::Items;
use horizon_core::nbt::{Compound, Tag};
use std::borrow::Cow;

pub use crate::rules::food_stock::{EXCESS, WANT};
use crate::rules::food_stock as rule;

/// Points de nourriture mangés et non encore digérés.
#[derive(Component, Clone, Copy, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct Food(pub i32);

/// `Villager.addAdditionalSaveData` writes `FoodLevel` as a byte; `readAdditionalSaveData` reads it with `getByteOr`, so an int or a
/// long keeps its low 8 bits (`(byte)(value & 255)`).
pub const FIELDS: &[Field] = &[Field { key: Cow::Borrowed("FoodLevel"), write: write_food, read: read_food, accepts: accept::any, owner: Some(owner::<Food>) }];

fn write_food(w: &World, e: Entity, c: &mut Compound) {
    if let Some(f) = w.get::<Food>(e) {
        c.insert("FoodLevel".into(), Tag::Byte(f.0 as i8));
    }
}

fn read_food(w: &mut World, e: Entity, t: &Tag) -> bool {
    let v = match t {
        Tag::Byte(v) => i64::from(*v),
        Tag::Short(v) => i64::from(*v),
        Tag::Int(v) => i64::from(*v),
        Tag::Long(v) => *v,
        _ => return false,
    };
    w.entity_mut(e).insert(Food(i32::from((v & 255) as u8 as i8)));
    true
}

/// Le registre d'objets du monde (la donnée `VILLAGER_FOOD`).
pub fn items(w: &World) -> &'static Items {
    w.resource::<Level>().mw.0.items.expect("item data")
}
pub fn level(w: &World, e: Entity, items: &Items) -> i32 {
    rule::total(w.get::<Food>(e).map_or(0, |f| f.0), w.get::<Stock>(e).map_or(0, |s| inv::points(items, s)))
}
pub fn can_breed(w: &World, e: Entity, items: &Items) -> bool {
    rule::can_breed(level(w, e, items))
}
pub fn wants_more(w: &World, e: Entity, items: &Items) -> bool {
    rule::wants_more(level(w, e, items))
}
pub fn has_excess(w: &World, e: Entity, items: &Items) -> bool {
    rule::has_excess(level(w, e, items))
}
/// `eatAndDigest` : digère une part de la réserve.
pub fn digest(w: &mut World, e: Entity) {
    if let Some(mut f) = w.get_mut::<Food>(e) {
        f.0 = rule::digested(f.0);
    }
}
