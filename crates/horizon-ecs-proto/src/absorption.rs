//! The absorption hearts of a living entity (`LivingEntity.absorptionAmount`, lot 75). A sparse component, absent at 0. Written by
//! event (golden apple, effect, mod write), never by tick: the decay of an effect belongs to the effects lot (43).
use bevy_ecs::prelude::*;

/// The absorption, in health points. Absent when it is 0.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
#[component(storage = "SparseSet")]
pub struct Absorption(pub f32);

/// Writes the absorption, not below 0; 0 removes the component. The cap of `getMaxAbsorption` is not modelled here.
pub fn set(world: &mut World, e: Entity, amount: f32) {
    let Ok(mut em) = world.get_entity_mut(e) else { return };
    if amount > 0.0 {
        em.insert(Absorption(amount));
    } else {
        em.remove::<Absorption>();
    }
}

/// Takes `damage` off the absorption first and returns the damage left for the health.
pub fn absorb(world: &mut World, e: Entity, damage: f32) -> f32 {
    let Some(a) = world.get::<Absorption>(e).map(|a| a.0) else { return damage };
    let taken = a.min(damage);
    set(world, e, a - taken);
    damage - taken
}
