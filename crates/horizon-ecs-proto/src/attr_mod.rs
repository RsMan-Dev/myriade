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

/// One modifier: its id, its amount and how it applies.
#[derive(Clone, Debug, PartialEq)]
pub struct Modifier {
    pub id: String,
    pub amount: f64,
    pub operation: Operation,
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
