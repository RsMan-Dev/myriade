//! Attribute modifiers that a behaviour sets on an entity by id. The armour is the only attribute set so far: a modifier adds its amount to
//! `Armor`, and setting the same id again replaces its amount.
use crate::equipment::Armor;
use bevy_ecs::prelude::*;

/// The armour modifiers of an entity, by id and amount.
#[derive(Component, Clone, Default, Debug)]
pub struct ArmorModifiers(pub Vec<(&'static str, f64)>);

/// Sets the armour modifier `id` to `amount`, or removes it (`None`). `Armor` is made when the entity has none.
pub fn set_armor_modifier(w: &mut World, e: Entity, id: &'static str, amount: Option<f64>) {
    let mut mods = w.get::<ArmorModifiers>(e).cloned().unwrap_or_default();
    let old = mods.0.iter().find(|(k, _)| *k == id).map_or(0.0, |(_, v)| *v);
    mods.0.retain(|(k, _)| *k != id);
    if let Some(a) = amount {
        mods.0.push((id, a));
    }
    if w.get::<Armor>(e).is_none() {
        w.entity_mut(e).insert(Armor::default());
    }
    w.get_mut::<Armor>(e).unwrap().armor += (amount.unwrap_or(0.0) - old) as f32;
    w.entity_mut(e).insert(mods);
}
