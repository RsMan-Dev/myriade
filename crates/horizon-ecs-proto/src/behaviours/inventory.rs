//! L'inventaire du villageois (`Villager.inventory`, ce qu'il porte sans les emplacements d'armure) : un composant
//! `Stock` par item (`minecraft:wheat`...). Absent tant qu'il est vide : un villageois sans rien n'a pas le composant.
use crate::brain::*;
use bevy_ecs::prelude::Component;
use std::collections::BTreeMap;

/// Item name to count, counts above zero only.
#[derive(Component, Default, Clone, Debug)]
pub struct Stock(pub BTreeMap<&'static str, i32>);

pub fn count(w: &World, e: Entity, item: &str) -> i32 {
    w.get::<Stock>(e).and_then(|s| s.0.get(item).copied()).unwrap_or(0)
}

pub fn add(w: &mut World, e: Entity, item: &'static str, n: i32) {
    if n <= 0 {
        return;
    }
    if w.get::<Stock>(e).is_none() {
        w.entity_mut(e).insert(Stock::default());
    }
    *w.get_mut::<Stock>(e).unwrap().0.entry(item).or_insert(0) += n;
}

/// Takes `n` of `item` if there are that many; the component goes when it is empty again.
pub fn take(w: &mut World, e: Entity, item: &str, n: i32) -> bool {
    if count(w, e, item) < n {
        return false;
    }
    let empty = {
        let mut s = w.get_mut::<Stock>(e).unwrap();
        let left = {
            let c = s.0.get_mut(item).unwrap();
            *c -= n;
            *c
        };
        if left == 0 {
            s.0.remove(item);
        }
        s.0.is_empty()
    };
    if empty {
        w.entity_mut(e).remove::<Stock>();
    }
    true
}
