//! A player's item in hand traded for another on a creature: a bucket for milk on a cow, a bowl for stew on a mooshroom, a water bucket for a bucket of fish
//! on a fish (the creature goes: `takes_creature`). A RULE of the species (`HELD_SWAP`), `Mob.mobInteract` of the adult. What the pair is, is a parameter;
//! the item leaves the hand and the other takes its place (`Online`: the bench's player has the one hand). Not ported: the sound, the baby's refusal
//! beyond `adult_only`, the fish's data in the bucket (variant, name), the milk's cooldown, the sheep's dye and the shears (`farm::SHEAR` does the shears).
use crate::cow::*;
use crate::goals::*;
use crate::rules::{no_attrs, params, Rule};
use bevy_ecs::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct SwapParams {
    pub from: &'static str,
    pub to: &'static str,
    pub adult_only: bool,
    /// The creature is taken into the item (a fish in its bucket).
    pub takes_creature: bool,
}
pub static HELD_SWAP: Rule = Rule { name: "held_swap", equip: |_, _, _| {}, attrs: no_attrs, interact };
fn interact(w: &mut World, e: Entity, p: &'static (dyn std::any::Any + Sync), i: &crate::behaviours::Interaction) -> bool {
    let p = *params::<SwapParams>(p);
    if i.item == 0 || !crate::items::is(p.from, i.item) || (p.adult_only && w.get::<GrowAt>(e).is_some()) {
        return false;
    }
    let to = crate::items::item_id(p.to);
    let Some(pl) = w.resource::<Online>().get(i.player) else { return false };
    if to == 0 {
        return false;
    }
    let hand = if pl.main == i.item { Player { main: to, ..pl } } else { Player { off: to, ..pl } };
    w.resource_mut::<Online>().set(hand);
    if p.takes_creature {
        w.entity_mut(e).despawn();
    }
    true
}
