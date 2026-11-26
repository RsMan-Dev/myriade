//! The rule `DUPLICATE_ITEM` (`Allay.mobInteract`, `duplicateAllay`): an item of the tag, given to an entity that dances and is not on
//! cooldown, makes a copy of it at its place; both wait `cooldown` ticks, the item is used up, with a sound and the entity event 18 (hearts).
//! The cooldown is a deadline on the entity's own clock (`Cooldown`), not a counter that ticks. Not done: `setPersistenceRequired` on the copy
//! (the persistence flag is not read here), the sound's source (neutral).
use crate::cow::*;
use crate::goals::Events;
use crate::rules::{no_attrs, params, Rule};
use bevy_ecs::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct DuplicateParams {
    pub item: &'static str,
    pub cooldown: u64,
    pub sound: &'static str,
    pub event: u8,
}
impl DuplicateParams {
    pub const ALLAY: Self = Self { item: "#minecraft:duplicates_allays", cooldown: 6000, sound: "minecraft:block.amethyst_block.chime", event: 18 };
}

/// The tick (own clock) from which the entity may duplicate again.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Cooldown(pub u64);

pub static DUPLICATE_ITEM: Rule = Rule { name: "duplicate_item", equip: |_, _, _| {}, attrs: no_attrs, interact };

fn interact(w: &mut World, e: Entity, p: &'static (dyn std::any::Any + Sync), i: &crate::behaviours::Interaction) -> bool {
    let p = *params::<DuplicateParams>(p);
    if i.item == 0 || !crate::items::is(p.item, i.item) || w.get::<crate::behaviours::event_listen::Dancing>(e).is_none() {
        return false;
    }
    let now = now_of(w, e);
    if w.get::<Cooldown>(e).is_some_and(|c| c.0 > now) {
        return false;
    }
    let Some(at) = w.get::<Pos>(e).map(|q| q.0) else { return false };
    let seed = w.get_mut::<Rng>(e).unwrap().next_u32() as u64;
    let copy = crate::cow::spawn_like(w, e, at.x, at.z, seed);
    if let Some(mut q) = w.get_mut::<Pos>(copy) {
        q.0 = at;
    }
    crate::brain::start(w, copy);
    let until = now + p.cooldown;
    w.entity_mut(e).insert(Cooldown(until));
    let copy_now = now_of(w, copy);
    w.entity_mut(copy).insert(Cooldown(copy_now + p.cooldown));
    let ev = &mut w.resource_mut::<Events>().host;
    ev.entity_event(e, p.event);
    ev.sound(p.sound, [at.x, at.y, at.z], 2.0, 1.0);
    crate::give_item::take_from_hand(w, i);
    true
}
