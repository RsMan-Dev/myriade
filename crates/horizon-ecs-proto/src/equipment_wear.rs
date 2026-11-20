//! Wear of the armour when a mob is hurt (`LivingEntity.doHurtEquipment`: `max(1, damage / 4)` per worn piece that wears when hurt, then
//! `ItemStack.hurtAndBreak`), and the attack the main hand adds. The damage of each piece is kept in `Wear`, absent until a piece takes some;
//! a piece that breaks leaves its slot and its wear is reset. The caller plays the break (the event of the slot, the sound).
//! Not here: Unbreaking (`processDurabilityChange`, which needs the enchantments' data).
use crate::equipment::{set_slot, take_slot};
use crate::goals::{equipped, Slot};
use crate::items;
use bevy_ecs::prelude::*;

/// The armour slots, in the order `Wear` keeps them.
pub const ARMOR: [Slot; 4] = [Slot::Head, Slot::Chest, Slot::Legs, Slot::Feet];

/// The damage each armour piece took, by `ARMOR` index.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Wear(pub [i32; 4]);

/// The amount a hurt of `damage` wears off each piece: none for no damage (or NaN), else `max(1, damage / 4)`.
pub fn wear_amount(damage: f32) -> i32 {
    if damage.is_nan() || damage <= 0.0 {
        0
    } else {
        (damage / 4.0).max(1.0) as i32
    }
}

/// `ItemStack.hurtAndBreak` on one piece: its damage takes `amount`; true when it reaches `max`, the piece breaks.
pub fn hurt(damage: &mut i32, amount: i32, max: i32) -> bool {
    *damage += amount;
    *damage >= max
}

/// `LivingEntity.doHurtEquipment` over the armour of `e` for a hurt of `damage`. The pieces that break leave their slot; their slots are
/// returned.
pub fn hurt_armor(w: &mut World, e: Entity, damage: f32) -> Vec<Slot> {
    let amount = wear_amount(damage);
    if amount == 0 {
        return Vec::new();
    }
    let mut wear = w.get::<Wear>(e).copied().unwrap_or_default();
    let mut broke = Vec::new();
    let mut any = false;
    for (i, slot) in ARMOR.into_iter().enumerate() {
        let max = crate::equipment_items::durability_when_hurt(equipped(w, e, slot));
        if max <= 0 {
            continue;
        }
        any = true;
        if hurt(&mut wear.0[i], amount, max) {
            take_slot(w, e, slot);
            wear.0[i] = 0;
            broke.push(slot);
        }
    }
    if any {
        w.entity_mut(e).insert(wear);
    }
    broke
}

/// The `ARMOR` index of a slot, if it is an armour slot.
pub fn armor_index(slot: Slot) -> Option<usize> {
    match slot {
        Slot::Head => Some(0),
        Slot::Chest => Some(1),
        Slot::Legs => Some(2),
        Slot::Feet => Some(3),
        _ => None,
    }
}

/// Puts a new piece in a slot: an armour piece starts with no wear.
pub fn put_armor(w: &mut World, e: Entity, slot: Slot, item: u32) {
    if let Some(i) = armor_index(slot) {
        let mut wear = w.get::<Wear>(e).copied().unwrap_or_default();
        wear.0[i] = 0;
        w.entity_mut(e).insert(wear);
    }
    set_slot(w, e, slot, item);
}

/// The attack the main hand adds: its attack damage (`items::attack_damage_of`), 0 empty-handed. The base attack of the species is not here.
pub fn hand_attack(w: &World, e: Entity) -> f64 {
    items::attack_damage_of(equipped(w, e, Slot::Main))
}
