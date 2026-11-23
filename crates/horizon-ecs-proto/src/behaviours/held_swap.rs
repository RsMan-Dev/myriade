//! A player's item in hand traded for another on a creature: a bucket for milk on a cow, a bowl for stew on a mooshroom, a water bucket for a bucket of fish
//! on a fish (the creature goes: `takes_creature`). A RULE of the species (`HELD_SWAP`), `Mob.mobInteract` of the adult. What the pair is, is a parameter;
//! the item leaves the hand and the other takes its place (`Online`: the bench's player has the one hand); in creative the item stays. The host loses the item and gets the other by `HostEvent::HeldSwap`.
//! Not ported: the baby's refusal beyond `adult_only`, the fish's data in the bucket (variant, name), the sheep's dye (`behaviours::shear` does the shears).
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
    /// The sound of the swap, played where the creature is.
    pub sound: &'static str,
    /// Another sound when the test holds on the creature (the screaming goat's milk).
    pub alt: Option<(fn(&World, Entity) -> bool, &'static str)>,
}
/// The item of `i` leaves the player's hand and `to` takes its place: in the mirror (`Online`, by id) and on the host (`HostEvent::HeldSwap`,
/// which carries the stack's data). False when the player is not online or `to` is no item.
pub fn swap_in_hand(w: &mut World, i: &crate::behaviours::Interaction, to: myriade_engine_lib::items::ItemStack) -> bool {
    let id = crate::items::item_id(&to.item);
    let Some(pl) = w.resource::<Online>().get(i.player) else { return false };
    if id == 0 {
        return false;
    }
    // Creative (`createFilledResult`): the item stays in the hand, the host adds the result when the inventory lacks it.
    if !w.get_resource::<crate::used_stack::UsedStack>().is_some_and(|u| u.creative) {
        let hand = if pl.main == i.item { Player { main: id, ..pl } } else { Player { off: id, ..pl } };
        w.resource_mut::<Online>().set(hand);
    }
    w.resource_mut::<Events>().host.held_swap(i.player, i.item, to);
    true
}
pub static HELD_SWAP: Rule = Rule { name: "held_swap", equip: |_, _, _| {}, attrs: no_attrs, interact };
fn interact(w: &mut World, e: Entity, p: &'static (dyn std::any::Any + Sync), i: &crate::behaviours::Interaction) -> bool {
    let p = *params::<SwapParams>(p);
    if i.item == 0 || !crate::items::is(p.from, i.item) || (p.adult_only && w.get::<GrowAt>(e).is_some()) {
        return false;
    }
    if !swap_in_hand(w, i, myriade_engine_lib::items::ItemStack::new(p.to, 1)) {
        return false;
    }
    let sound = p.alt.filter(|(test, _)| test(w, e)).map_or(p.sound, |(_, s)| s);
    crate::behaviours::act_phase::sound(w, e, sound, 1.0, 1.0);
    if p.takes_creature {
        w.entity_mut(e).despawn();
    }
    true
}
