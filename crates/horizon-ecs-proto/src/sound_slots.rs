//! The sounds a mod declares for an entity type (`sounds` of its declaration: ambient, hurt, death, step). One table per type, kept in a
//! resource and replaced on each load or reload; an entity holds nothing. A sound asks `sound`: the species' own sound unless the table
//! overrides the slot, and nothing when the slot is muted.
use crate::area::species_name;
use bevy_ecs::prelude::*;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Slot {
    #[default]
    Inherit,
    Mute,
    Id(&'static str),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Overrides {
    pub ambient: Slot,
    pub hurt: Slot,
    pub death: Slot,
    pub step: Slot,
}

#[derive(Resource, Default)]
pub struct SoundSlots(HashMap<&'static str, Overrides>);

/// Replaces the table of every declared type (a reload drops the types it no longer declares).
pub fn replace(w: &mut World, table: HashMap<&'static str, Overrides>) {
    w.insert_resource(SoundSlots(table));
}

/// The sound of `slot` for `e`: `vanilla` when inherited (None when the species' own sound is not named here), the declared id, or
/// None when muted. `Step` is `|o| o.step`: the step sound is read by the movement module, which plays it.
pub fn sound(w: &World, e: Entity, slot: fn(&Overrides) -> Slot, vanilla: Option<&'static str>) -> Option<&'static str> {
    let over = w.get_resource::<SoundSlots>().and_then(|t| t.0.get(species_name(w, e)));
    match over.map_or(Slot::Inherit, slot) {
        Slot::Inherit => vanilla,
        Slot::Id(id) => Some(id),
        Slot::Mute => None,
    }
}

/// `LivingEntity.playHurtSound`: the declared hurt voice of `e`, at its place with its own pitch (`tear_down::voice`).
pub fn hurt(w: &mut World, e: Entity) {
    crate::behaviours::ambient_sound::reset_on_hurt(w, e);
    if let Some(name) = sound(w, e, |o| o.hurt, None) {
        crate::tear_down::voice(w, e, name);
    }
}

/// `LivingEntity.die` (`makeSound(getDeathSound)`): the declared death voice of `e`.
pub fn death(w: &mut World, e: Entity) {
    if let Some(name) = sound(w, e, |o| o.death, None) {
        crate::tear_down::voice(w, e, name);
    }
}
