//! Inventaire du villageois (`SimpleContainer(8)`, `Villager.inventory`) : huit emplacements, composant à part, absent tant qu'il
//! est vide. Les points de nourriture viennent des données (`villager::food_points`). Ramasser, manger, partager.
use crate::behaviours::food_stock::{self as stock, Food};
use crate::brain::*;
use horizon_client_lib::{items::Items, mobs::villager::food_points};

pub const SLOTS: usize = 8;
/// Taille d'une pile (`Item.getMaxStackSize`) : la donnée des objets est lue ailleurs, ici 64 (le blé, les graines, le pain).
const STACK: u8 = 64;

#[derive(Component, Clone, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct Stock(pub Vec<(String, u8)>);

/// Points de nourriture de l'inventaire.
pub fn points(items: &Items, s: &Stock) -> i32 {
    s.0.iter().map(|(name, n)| food_points(items, name) * i32::from(*n)).sum()
}

/// Met `count` objets `item` dans l'inventaire : d'abord les piles existantes, puis des emplacements libres. Rend ce qui n'entre pas.
pub fn insert(w: &mut World, e: Entity, item: &str, mut count: u8) -> u8 {
    let mut s = w.get::<Stock>(e).cloned().unwrap_or_default();
    for (_, n) in s.0.iter_mut().filter(|(name, _)| name == item) {
        let fill = (STACK - *n).min(count);
        *n += fill;
        count -= fill;
    }
    while count > 0 && s.0.len() < SLOTS {
        let put = count.min(STACK);
        s.0.push((item.to_string(), put));
        count -= put;
    }
    if s.0.is_empty() {
        w.entity_mut(e).remove::<Stock>();
    } else {
        w.entity_mut(e).insert(s);
    }
    count
}

/// Retire un objet d'aliment (qui a des points) : rend son nom, ou `None` s'il n'y en a pas.
fn take_food(w: &mut World, e: Entity, items: &Items) -> Option<String> {
    let mut s = w.get::<Stock>(e)?.clone();
    let i = s.0.iter().position(|(name, _)| food_points(items, name) > 0)?;
    let name = s.0[i].0.clone();
    s.0[i].1 -= 1;
    if s.0[i].1 == 0 {
        s.0.remove(i);
    }
    if s.0.is_empty() {
        w.entity_mut(e).remove::<Stock>();
    } else {
        w.entity_mut(e).insert(s);
    }
    Some(name)
}

/// Mange un objet si la réserve est sous 12 (`hungry`), puis rend si le villageois peut se reproduire (il est alors volontaire).
pub fn make_willing(w: &mut World, e: Entity, items: &Items) -> bool {
    if stock::wants_more(w, e, items) {
        if let Some(name) = take_food(w, e, items) {
            let p = food_points(items, &name);
            let food = w.get::<Food>(e).map_or(0, |f| f.0);
            w.entity_mut(e).insert(Food(food + p));
        }
    }
    stock::can_breed(w, e, items)
}

/// Donne un objet d'aliment de `a` à `b` quand `a` a de quoi en donner (plus de 24 points) et `b` en veut (moins de 12).
pub fn share(w: &mut World, a: Entity, b: Entity, items: &Items) -> bool {
    if !stock::has_excess(w, a, items) || !stock::wants_more(w, b, items) {
        return false;
    }
    let Some(name) = take_food(w, a, items) else { return false };
    let left = insert(w, b, &name, 1);
    left == 0
}
