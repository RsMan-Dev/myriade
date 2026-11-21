//! The use of ONE item of the stack a player holds, whatever it is: a tool that wears loses a point of durability (`HostEvent::HeldWorn`,
//! `ItemStack.hurtAndBreak(1)`), any other item is used up (`give_item::take_from_hand`, `ItemStack.shrink(1)`). Nothing here names a species.
use crate::behaviours::Interaction;
use crate::equipment_items::durability_of;
use crate::give_item::take_from_hand;
use crate::goals::Events;
use bevy_ecs::prelude::*;

/// `amount` points of wear on the item the player holds.
pub fn wear_in_hand(w: &mut World, i: &Interaction, amount: u32) {
    w.resource_mut::<Events>().host.held_worn(i.player, i.item, amount);
}
/// Wear of 1 when the item has a durability, else one item less.
pub fn spend_one(w: &mut World, i: &Interaction) {
    if durability_of(i.item) > 0 {
        wear_in_hand(w, i, 1);
    } else {
        take_from_hand(w, i);
    }
}
