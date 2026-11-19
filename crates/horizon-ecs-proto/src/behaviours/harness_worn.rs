//! The harness a happy ghast wears (`happy_ghast.rs` `wearing_harness`, `harnessed_adult`, `harness_texture`): an adult with no harness
//! takes an item whose name ends with `_harness` from a player's hand, and keeps its name for the texture
//! (`entity/equipment/happy_ghast_body/<name>`). A baby (`GrowAt`) never takes one. The restriction radius and the ride depend on it.
//! DOUTE : la consommation de l'objet et le son des lunettes (`harness_goggles_down/up`) sont du cadre (socle).
//! DOUTE : `goals::item_name` (identifiant vers nom) n'existe pas dans le cadre ; `is_harness` en dépend.
use crate::behaviours::Interaction;
use crate::cow::GrowAt;
use crate::rules::{no_attrs, no_interact, Rule};
use bevy_ecs::prelude::*;
use std::sync::Arc;

/// The worn harness, by item name.
#[derive(Component, Clone, Debug)]
pub struct Harness {
    pub item: Arc<str>,
}

pub static RULE: Rule = Rule { name: "happy_ghast_harness", equip, attrs: no_attrs, interact };

fn equip(_: &mut World, _: Entity, _: &(dyn std::any::Any + Sync)) {}

/// `harnessed_adult`: wearing a harness and not a baby.
pub fn harnessed_adult(w: &World, e: Entity) -> bool {
    w.get::<Harness>(e).is_some() && w.get::<GrowAt>(e).is_none()
}

/// A player offers an item to an adult without a harness: the harness is worn if the item is one.
fn interact(w: &mut World, e: Entity, p: &(dyn std::any::Any + Sync), i: &Interaction) -> bool {
    if w.get::<Harness>(e).is_some() || w.get::<GrowAt>(e).is_some() || !is_harness(i.item) {
        return no_interact(w, e, p, i);
    }
    let item: Arc<str> = Arc::from(crate::items::item_name(i.item).unwrap_or_default());
    w.entity_mut(e).insert(Harness { item });
    true
}
fn is_harness(item: u32) -> bool {
    crate::items::item_name(item).is_some_and(|n| n.ends_with("_harness"))
}
