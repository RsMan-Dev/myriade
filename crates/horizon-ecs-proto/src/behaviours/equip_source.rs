//! The pickup of a mob's gear, on the event of one item coming into its reach (`item_in_reach`, called by the item's own move, not by a scan
//! of the mob's box every tick). The mob takes one of the stack when `wants_to_pick_up` (the species, the item) and `equip_decision::decide`
//! agree; the piece it replaces drops at its death with the roll of `equipment_drop`, unless the mob already looted that slot.
//! Needs the socle's `equipment::set_slot` and `items` (branch `ecs/equipment`).
use super::equip_decision::decide;
use crate::equipment::set_slot;
use crate::equipment_wear::put_armor;
use crate::goals::{equipped, set_main_hand, Slot};
use crate::item;
use crate::items;
use bevy_ecs::prelude::*;
use horizon_client_lib::mobs::{
    equip::{wants_to_pick_up, ItemTags},
    Species,
};

/// What the mob is, for the pickup: the species, whether it is a baby or a passenger, and the item tags of its preferred weapons.
pub struct Taker {
    pub species: Species,
    pub baby: bool,
    pub passenger: bool,
    pub preferred_tag: Option<&'static str>,
}

/// The item `item` came into the reach of `mob`: takes one of its stack when it wants and can use it. Returns the slot taken, if any.
pub fn item_in_reach(w: &mut World, mob: Entity, item: Entity, taker: &Taker) -> Option<Slot> {
    let name = w.get::<item::Item>(item)?.stack.item.to_string();
    let id = items::item_id(&name);
    if id == 0 || !item::pickable(w, item) {
        return None;
    }
    let tags = ItemTags { spear: items::has_tag("minecraft:spears", id), egg: items::has_tag("minecraft:eggs", id), disliked_weapon: false };
    if !wants_to_pick_up(taker.species, taker.baby, taker.passenger, &name, tags) {
        return None;
    }
    let slot = decide(id, |s| equipped(w, mob, s), taker.preferred_tag)?;
    let taken = item::pick_up(w, mob, item, 1)?;
    match slot {
        Slot::Head | Slot::Chest | Slot::Legs | Slot::Feet => put_armor(w, mob, slot, id),
        _ => set_slot(w, mob, slot, id),
    }
    if slot == Slot::Main {
        set_main_hand(w, mob, Some(taken));
    }
    // The piece it replaced stays on the mob until its death roll (`equipment_drop`): nothing is spawned here.
    Some(slot)
}
