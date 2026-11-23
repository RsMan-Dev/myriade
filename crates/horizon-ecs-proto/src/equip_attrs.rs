//! The attribute modifiers of the items an entity wears or holds (`LivingEntity.collectEquipmentChanges`, `ItemAttributeModifiers`), applied
//! and removed AT THE EVENT of a slot change (put on, taken off, broken, loaded), never per tick. The host owns the table (item data, a stack's
//! own component, mods) and gives it once at the level's creation as `ItemModifiers`; P only keeps what the slots grant, in a sparse component
//! that the entity has while it wears something that modifies an attribute. These modifiers are transient: they are not saved.
use crate::attr_mod::Modifier;
use bevy_ecs::prelude::*;
use myriade_engine_lib::items::ItemStack;

/// The modifiers `stack` grants in the slot of ordinal `slot` (`EquipmentSlot` order), as (attribute without `minecraft:`, modifier); the
/// host has already kept the rows whose slot group matches.
pub type ItemModifiersFn = Box<dyn Fn(&ItemStack, usize) -> Vec<(String, Modifier)> + Send + Sync>;

/// The host's table, set once at the level's creation; without it (the pure tests) an item grants nothing.
#[derive(Resource)]
pub struct ItemModifiers(pub ItemModifiersFn);

/// What the slots of an entity grant: (slot ordinal, attribute, modifier).
#[derive(Component, Clone, Debug, Default, PartialEq)]
#[component(storage = "SparseSet")]
pub struct WornModifiers(pub Vec<(usize, String, Modifier)>);

/// `removeModifier` of the old item then `addTransientModifier` of the new one, for `slot`. The component goes with the last modifier.
pub fn changed(w: &mut World, e: Entity, slot: usize, to: Option<&ItemStack>) {
    let rows = match (to, w.get_resource::<ItemModifiers>()) {
        (Some(s), Some(t)) => (t.0)(s, slot),
        _ => Vec::new(),
    };
    if rows.is_empty() && w.get::<WornModifiers>(e).is_none() {
        return;
    }
    let mut list = w.get::<WornModifiers>(e).cloned().unwrap_or_default();
    let mut names: Vec<String> = list.0.iter().filter(|r| r.0 == slot).map(|r| r.1.clone()).collect();
    names.extend(rows.iter().map(|r| r.0.clone()));
    names.sort_unstable();
    names.dedup();
    list.0.retain(|r| r.0 != slot);
    list.0.extend(rows.into_iter().map(|(a, m)| (slot, a, m)));
    if list.0.is_empty() {
        w.entity_mut(e).remove::<WornModifiers>();
    } else {
        w.entity_mut(e).insert(list);
    }
    for n in names {
        crate::attr_mod::touched(w, e, &n);
    }
}

/// The modifiers the worn items give to attribute `name` (without `minecraft:`).
pub fn of_attribute(w: &World, e: Entity, name: &str) -> Vec<Modifier> {
    w.get::<WornModifiers>(e).map_or_else(Vec::new, |l| l.0.iter().filter(|r| r.1 == name).map(|r| r.2.clone()).collect())
}
