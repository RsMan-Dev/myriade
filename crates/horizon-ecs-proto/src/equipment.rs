//! What an entity wears and carries, written through ONE service: `set_slot(w, e, slot, item)` writes the slot of `Equipment` (created if the entity wore
//! nothing), raises `Events.equipment_changed`, and keeps the state derived from the slots current: the saddle slot is `Mount.saddled`, the worn pieces
//! are `Armor` (the sum of what each piece gives, read from the item's own `attribute_modifiers`, `items::armor_of`). A hit takes the armor into
//! account (`absorb`, `CombatRules.getDamageAfterAbsorb`). Putting equipment on is an interaction, the rule `EQUIP` (a player's item in hand goes to
//! the slot it is made for, on an entity that accepts it), taking it off a service (`take_slot`). Nothing here is about one species.
use crate::cow::*;
use crate::goals::*;
use crate::items::{self, worn_in};
use crate::rules::equipment as rule;
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
    // A slot whose whole stack `equip_stack` holds took its modifiers there; the bare item's table applies otherwise.
    let ordinal = match slot {
        Slot::Main => 0,
        Slot::Off => 1,
        Slot::Feet => 2,
        Slot::Legs => 3,
        Slot::Chest => 4,
        Slot::Head => 5,
        Slot::Body => 6,
        Slot::Saddle => 7,
    };
    if crate::equip_stack::get(w, e, ordinal).is_none() {
        let bare = items::item_name(item).map(|n| horizon_client_lib::items::ItemStack::new(n, 1));
        crate::equip_attrs::changed(w, e, ordinal, bare.as_ref());
    }
    w.resource_mut::<Events>().equipment_changed.push(e);
    // The goals that depend on what is held (admire, crossbow, melee) are asked again at the next tick, by the same list as the entered ones
    // (no read of the events in a system that runs every tick).
    if w.get::<crate::goals::GoalSet>(e).is_some() {
        w.resource_mut::<crate::goals::Entered>().0.push(e);
    }
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
/// Recomputes `Armor` from the species' base, the worn pieces, the `ArmorModifiers` and what `/attribute` wrote (`AttrMods`: a base over
/// the species' and modifiers). Run when a piece changes and when `AttrMods` changes for `armor` or `armor_toughness` (`attr_mod::touched`).
pub(crate) fn refresh_armor(w: &mut World, e: Entity) {
    let q = w.get::<Equipment>(e).copied().unwrap_or_default();
    let base = w.get::<SpeciesId>(e).map_or((0.0, 0.0), |s| w.resource::<SpeciesTable>().0[s.0 as usize].armor);
    // With the host's item table (`ItemModifiers`) the worn rows (`WornModifiers`, a stack's own included) give what the pieces give.
    let host = w.contains_resource::<crate::equip_attrs::ItemModifiers>();
    let pieces = if host { [(0.0, 0.0); 5] } else { [q.head, q.chest, q.legs, q.feet, q.body].map(items::armor_of) };
    let (mut armor, mut toughness, mut worn) = rule::armor_totals(base, &pieces);
    let added: f64 = w.get::<crate::attributes::ArmorModifiers>(e).map_or(0.0, |m| m.0.iter().map(|x| x.1).sum());
    let written = |name: &str| w.get::<crate::attr_mod::AttrMods>(e).and_then(|m| m.0.iter().find(|x| x.0 == name).cloned());
    let (wa, wt) = (written("armor"), written("armor_toughness"));
    let rows = |name: &str| if host { crate::equip_attrs::of_attribute(w, e, name) } else { Vec::new() };
    let (ra, rt) = (rows("armor"), rows("armor_toughness"));
    worn |= !ra.is_empty() || !rt.is_empty();
    let custom = added != 0.0 || wa.is_some() || wt.is_some() || worn && host;
    if custom {
        let (pa, pt) = pieces.iter().fold((0.0, 0.0), |s, p| (s.0 + p.0, s.1 + p.1));
        let base_of = |x: &Option<(String, f64, Vec<crate::attr_mod::Modifier>)>, species: f32| x.as_ref().map(|x| x.1).filter(|b| !b.is_nan()).unwrap_or(f64::from(species));
        let mods = |x: &Option<(String, f64, Vec<crate::attr_mod::Modifier>)>, rows: Vec<crate::attr_mod::Modifier>| x.as_ref().map_or_else(Vec::new, |x| x.2.clone()).into_iter().chain(rows).collect::<Vec<_>>();
        armor = crate::attr_mod::value(base_of(&wa, base.0) + pa + added, &mods(&wa, ra)).clamp(0.0, 30.0).floor() as f32;
        toughness = crate::attr_mod::value(base_of(&wt, base.1) + pt, &mods(&wt, rt)).clamp(0.0, 20.0) as f32;
    }
    let now = Armor { armor, toughness };
    if !worn && !custom || now == Armor::default() && !custom {
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

/// The item a player (online, by id) wears or holds in a slot; 0 when nothing is there or the player is not online.
pub fn worn_by_player(w: &World, id: u32, slot: Slot) -> u32 {
    let Some(p) = w.get_resource::<Online>().and_then(|o| o.get(id)) else { return 0 };
    match slot {
        Slot::Head => p.head,
        Slot::Chest => p.chest,
        Slot::Legs => p.legs,
        Slot::Feet => p.feet,
        Slot::Main => p.main,
        Slot::Off => p.off,
        _ => 0,
    }
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
    /// `AbstractHorse.mobInteract` with a llama: a carpet on its body (no taming asked by the jar).
    pub const BODY: Self = Self { slots: &[Slot::Body], needs_tame: false };
    /// `AbstractNautilus.canUseSlot`: the saddle and the body armor, of a tame adult.
    pub const TAME_SADDLE_BODY: Self = Self { slots: &[Slot::Saddle, Slot::Body], needs_tame: true };
}
pub static EQUIP: Rule = Rule { name: "equip", equip: |_, _, _| {}, attrs: no_attrs, interact };
/// `Mob.mobInteract` of the saddle and the body armor: an adult (tame, if it must be) with the slot empty takes the item the player holds; the item leaves
/// the player's hand.
fn interact(w: &mut World, e: Entity, p: &'static (dyn std::any::Any + Sync), i: &crate::behaviours::Interaction) -> bool {
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
    crate::give_item::take_from_hand(w, i);
    true
}
