//! Réserve de nourriture (`Villager.foodLevel` + points de l'inventaire) : ce qui permet de se reproduire (`canBreed`, 12) et ce
//! qui est digéré. Les points de l'inventaire viennent de `food_inventory::points`, lus dans les données.
use crate::behaviours::food_inventory::{self as inv, Stock};
use bevy_ecs::prelude::*;
use crate::cow::Level;
use horizon_client_lib::items::Items;

pub use crate::rules::food_stock::{EXCESS, WANT};
use crate::rules::food_stock as rule;

/// Points de nourriture mangés et non encore digérés.
#[derive(Component, Clone, Copy, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct Food(pub i32);

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
