//! Lectures et retraits à l'unité sur l'inventaire du villageois (`food_inventory::Stock`, le seul : huit emplacements, absent
//! tant qu'il est vide).
use super::food_inventory::{insert, Stock};
use bevy_ecs::prelude::{Entity, World};

pub fn count(w: &World, e: Entity, item: &str) -> i32 {
    w.get::<Stock>(e).map_or(0, |s| s.0.iter().filter(|(n, _)| n == item).map(|(_, c)| i32::from(*c)).sum())
}

pub fn add(w: &mut World, e: Entity, item: &str, n: u8) -> u8 {
    insert(w, e, item, n)
}

/// Takes `n` of `item` if there are that many; the component goes when it is empty again.
pub fn take(w: &mut World, e: Entity, item: &str, n: i32) -> bool {
    if count(w, e, item) < n {
        return false;
    }
    let mut s = w.get::<Stock>(e).cloned().unwrap_or_default();
    let mut left = n;
    for (_, c) in s.0.iter_mut().filter(|(name, _)| name == item) {
        let t = left.min(i32::from(*c));
        *c -= t as u8;
        left -= t;
    }
    s.0.retain(|(_, c)| *c > 0);
    if s.0.is_empty() {
        w.entity_mut(e).remove::<Stock>();
    } else {
        w.entity_mut(e).insert(s);
    }
    true
}
