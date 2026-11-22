//! The modifiers of a living entity's attributes (`AttributeInstance`, lot 75). A sparse component holding only what differs from the
//! species' configuration: a base written over it, and the modifiers. The value is computed on read, never kept.
use bevy_ecs::prelude::*;

/// How a modifier applies (`AttributeModifier.Operation`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    AddValue,
    AddMultipliedBase,
    AddMultipliedTotal,
}

/// One modifier: its id, its amount and how it applies. A permanent one is saved with the entity (`addPermanentModifier`), a
/// transient one is not (`addOrUpdateTransientModifier`).
#[derive(Clone, Debug, PartialEq)]
pub struct Modifier {
    pub id: String,
    pub amount: f64,
    pub operation: Operation,
    pub permanent: bool,
}

/// The attributes this entity changes: (attribute name without `minecraft:`, base written over the species', modifiers). A base that
/// was never written is NaN: the species' base applies.
#[derive(Component, Clone, Debug, Default, PartialEq)]
#[component(storage = "SparseSet")]
pub struct AttrMods(pub Vec<(String, f64, Vec<Modifier>)>);

/// The value of an attribute from its base and its modifiers, in vanilla's order (`AttributeInstance.calculateValue`): the added
/// values join the base; the base-multiplied amounts apply to that sum; the total multipliers apply one after another. The clamp to
/// the attribute's range is not done here.
pub fn value(base: f64, mods: &[Modifier]) -> f64 {
    let of = |op: Operation| mods.iter().filter(move |m| m.operation == op).map(|m| m.amount);
    let v1 = of(Operation::AddValue).fold(base, |acc, a| acc + a);
    let v2 = of(Operation::AddMultipliedBase).fold(v1, |acc, a| acc + v1 * a);
    of(Operation::AddMultipliedTotal).fold(v2, |acc, a| acc * (1.0 + a))
}

/// The entities whose attribute instances changed since the host last read them (`AttributeMap.getAttributesToSync`): raised by the
/// writes, emptied by `EcsWorld::drain_attribute_changes`. Empty at rest.
#[derive(Resource, Default, Debug)]
pub struct AttrDirty(pub std::collections::HashSet<Entity>);

/// The attribute `name` (without `minecraft:`) of `e` changed (an `AttrMods` write or a worn modifier): the components the rules read at a
/// hit (`Armor`, `Resist`, `MaxHealth`) are brought up to date here, once, and the entity is marked for the sync to the clients.
pub fn touched(w: &mut World, e: Entity, name: &str) {
    match name {
        "armor" | "armor_toughness" => crate::equipment::refresh_armor(w, e),
        "knockback_resistance" => {
            let (base, mods) = match w.get::<AttrMods>(e).and_then(|m| m.0.iter().find(|x| x.0 == name)) {
                Some(x) => (Some(x.1).filter(|b| !b.is_nan()), x.2.clone()),
                None => (None, Vec::new()),
            };
            let species = w
                .get::<crate::cow::SpeciesId>(e)
                .and_then(|s| w.get_resource::<crate::cow::SpeciesTable>().map(|t| t.0[s.0 as usize].knockback_resistance));
            if let Some(species) = species {
                let all: Vec<Modifier> = mods.into_iter().chain(crate::equip_attrs::of_attribute(w, e, name)).collect();
                let v = value(base.unwrap_or(species), &all).clamp(0.0, 1.0);
                match w.get_mut::<crate::behaviours::stare_freeze::Braced>(e) {
                    Some(mut b) => b.0 = v,
                    None => {
                        w.entity_mut(e).insert(crate::cow::Resist(v));
                    }
                }
            }
        }
        "attack_damage" => {
            let (base, mods) = match w.get::<AttrMods>(e).and_then(|m| m.0.iter().find(|x| x.0 == name)) {
                Some(x) => (Some(x.1).filter(|b| !b.is_nan()), x.2.clone()),
                None => (None, Vec::new()),
            };
            let species = w
                .get::<crate::cow::SpeciesId>(e)
                .and_then(|s| w.get_resource::<crate::cow::SpeciesTable>().map(|t| t.0[s.0 as usize].attack_damage));
            if let Some(species) = species {
                // The variant's added attack (`Rabbit.setVariant`) stays on top of the species' base.
                let add = match (w.get::<crate::behaviours::variant::Variant>(e), crate::behaviours::variant::table_of(w, e)) {
                    (Some(v), Some(t)) => t.traits.iter().find(|x| x.variant == v.0).map_or(0.0, |x| f64::from(x.damage_add)),
                    _ => 0.0,
                };
                let all: Vec<Modifier> = mods.into_iter().chain(crate::equip_attrs::of_attribute(w, e, name)).collect();
                let v = value(base.unwrap_or(f64::from(species) + add), &all) as f32;
                w.entity_mut(e).insert(crate::cow::AttackDamage(v));
            }
        }
        "max_health" => {
            if w.get::<crate::cow::SpeciesId>(e).is_some() && w.get::<crate::cow::MaxHealth>(e).is_some() {
                let max = crate::cow::max_health(w, e);
                w.entity_mut(e).insert(crate::cow::MaxHealth(max));
                if let Some(mut h) = w.get_mut::<crate::cow::Health>(e)
                    && h.0 > max
                {
                    h.0 = max;
                }
            }
        }
        _ => {}
    }
    w.get_resource_or_insert_with(AttrDirty::default).0.insert(e);
}
