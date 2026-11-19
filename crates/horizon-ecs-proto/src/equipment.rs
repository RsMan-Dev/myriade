//! What an entity wears and carries, written through ONE service: `set_slot(w, e, slot, item)` writes the slot of `Equipment` (created if the entity wore
//! nothing), raises `Events.equipment_changed`, and keeps the state derived from the slots current: the saddle slot is `Mount.saddled`, the worn pieces
//! are `Armor` (the sum of what each piece gives, read from the item's own `attribute_modifiers`, `items::armor_of`). A hit takes the armor into
//! account (`absorb`, `CombatRules.getDamageAfterAbsorb`). Putting equipment on is an interaction, the rule `EQUIP` (a player's item in hand goes to
//! the slot it is made for, on an entity that accepts it), taking it off a service (`take_slot`). Nothing here is about one species.
use crate::cow::*;
use crate::goals::*;
use crate::items::{self, worn_in};
use crate::rules::{no_attrs, params, Rule};
use bevy_ecs::prelude::*;

/// The armor an entity wears on top of its species' base (absent: only the base, `base_armor`): no byte for a creature that wears nothing.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Armor {
    pub armor: f32,
    pub toughness: f32,
}
fn slot_mut(q: &mut Equipment, slot: Slot) -> &mut u32 {
    match slot {
        Slot::Head => &mut q.head,
        Slot::Chest => &mut q.chest,
        Slot::Legs => &mut q.legs,
        Slot::Feet => &mut q.feet,
        Slot::Main => &mut q.main,
        Slot::Off => &mut q.off,
        Slot::Body => &mut q.body,
        Slot::Saddle => &mut q.saddle,
    }
}
/// `LivingEntity.setItemSlot`: the item (0: none) goes in the slot; the previous one is returned.
pub fn set_slot(w: &mut World, e: Entity, slot: Slot, item: u32) -> u32 {
    if w.get::<Equipment>(e).is_none() {
        w.entity_mut(e).insert(Equipment::default());
    }
    let old = std::mem::replace(slot_mut(&mut w.get_mut::<Equipment>(e).unwrap(), slot), item);
    if old == item {
        return old;
    }
    match slot {
        Slot::Saddle => {
            if let Some(mut m) = w.get_mut::<crate::mount::Mount>(e) {
                m.saddled = item != 0;
            }
        }
        Slot::Head | Slot::Chest | Slot::Legs | Slot::Feet | Slot::Body => refresh_armor(w, e),
        Slot::Main | Slot::Off => {}
    }
    w.resource_mut::<Events>().equipment_changed.push(e);
    old
}
/// `LivingEntity.setItemSlot` of the slot left empty; what it held.
pub fn take_slot(w: &mut World, e: Entity, slot: Slot) -> u32 {
    set_slot(w, e, slot, 0)
}
/// `Attributes.ARMOR` and `ARMOR_TOUGHNESS` of the type, the armor value floored.
pub fn base_armor(sp: &crate::cow::Species) -> (f32, f32) {
    (sp.armor.0.floor(), sp.armor.1)
}
fn refresh_armor(w: &mut World, e: Entity) {
    let q = *w.get::<Equipment>(e).unwrap();
    let base = w.get::<SpeciesId>(e).map_or((0.0, 0.0), |s| w.resource::<SpeciesTable>().0[s.0 as usize].armor);
    let (mut armor, mut toughness, mut worn) = (f64::from(base.0), f64::from(base.1), false);
    for it in [q.head, q.chest, q.legs, q.feet, q.body] {
        let (a, t) = items::armor_of(it);
        worn |= a != 0.0 || t != 0.0;
        armor += a;
        toughness += t;
    }
    // `armor::values`: the armor value is floored.
    let now = Armor { armor: armor.floor() as f32, toughness: toughness as f32 };
    if !worn {
        w.entity_mut(e).remove::<Armor>();
    } else if now == Armor::default() {
        w.entity_mut(e).remove::<Armor>();
    } else {
        w.entity_mut(e).insert(now);
    }
}
/// `CombatRules.getDamageAfterAbsorb`: what is left of `amount` once the armor and its toughness took their part.
pub fn absorb(amount: f32, armor: f32, toughness: f32) -> f32 {
    if armor <= 0.0 {
        return amount;
    }
    let t = 2.0 + toughness / 4.0;
    let effective = (armor - amount / t).clamp(armor * 0.2, 20.0);
    amount * (1.0 - effective / 25.0)
}
/// Whether a kind of hurt is stopped by armor (the others are in `#bypasses_armor`): a blow, flames, lava, cactus, an explosion; `on_fire` is in the tag, as data shows.
pub fn armor_applies(cause: Cause) -> bool {
    matches!(cause, Cause::Entity | Cause::Cactus | Cause::InFire | Cause::Lava | Cause::Blast)
}

/// What a player's item may be put on: which slots the entity accepts from a hand, and whether it must be tame and grown up.
#[derive(Clone, Copy, Debug)]
pub struct EquipParams {
    pub slots: &'static [Slot],
    pub needs_tame: bool,
}
impl EquipParams {
    /// A creature that takes a saddle (and armor on its body when it is a tame mount).
    pub const SADDLE: Self = Self { slots: &[Slot::Saddle], needs_tame: false };
    /// A horse takes its saddle once it is tame.
    pub const TAME_SADDLE: Self = Self { slots: &[Slot::Saddle], needs_tame: true };
}
pub static EQUIP: Rule = Rule { name: "equip", equip: |_, _, _| {}, attrs: no_attrs, interact };
/// `Mob.mobInteract` of the saddle and the body armor: an adult (tame, if it must be) with the slot empty takes the item the player holds; the item leaves
/// the player's hand.
fn interact(w: &mut World, e: Entity, p: &(dyn std::any::Any + Sync), i: &crate::behaviours::Interaction) -> bool {
    let p = *params::<EquipParams>(p);
    if i.item == 0 || w.get::<GrowAt>(e).is_some() {
        return false;
    }
    let Some(slot) = worn_in(i.item).filter(|s| p.slots.contains(s)) else { return false };
    if p.needs_tame && !w.get::<crate::tame::Owner>(e).is_some_and(|o| o.0.is_some()) {
        return false;
    }
    if equipped(w, e, slot) != 0 {
        return false;
    }
    set_slot(w, e, slot, i.item);
    if let Some(pl) = w.resource::<Online>().get(i.player) {
        let hand = Player { main: if pl.main == i.item { 0 } else { pl.main }, off: if pl.off == i.item && pl.main != i.item { 0 } else { pl.off }, ..pl };
        w.resource_mut::<Online>().set(hand);
    }
    true
}
