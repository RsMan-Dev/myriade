//! Which slot an item a mob meets goes to, and whether it beats what the mob wears or holds there: `Mob.canReplaceCurrentItem` on the facts of
//! the two items (`myriade_engine_lib::mobs::equip`), with the slot `getEquipmentSlotForItem` gives (the worn slot, else the main hand) and
//! the fallback of an armour piece that beats nothing it is worn with to an empty main hand (the socle's `look`). Pure: no world.
use crate::goals::Slot;
use crate::items;
use myriade_engine_lib::mobs::equip::{can_replace_current_item, Facts, Slot as MSlot};

/// The facts the choice reads of an item by id (armour, toughness, attack); enchantments, damage and name are not kept here yet.
pub fn facts(id: u32) -> Facts {
    let (armor, armor_toughness) = items::armor_of(id);
    Facts { armor, armor_toughness, attack_damage: items::attack_damage_of(id), ..Facts::default() }
}

/// The slot an item goes to: the slot it is worn in when that is an armour or the off hand, else the main hand.
pub fn slot_for(id: u32) -> Slot {
    match items::worn_in(id) {
        Some(s @ (Slot::Head | Slot::Chest | Slot::Legs | Slot::Feet | Slot::Off)) => s,
        _ => Slot::Main,
    }
}

fn mob_slot(s: Slot) -> MSlot {
    match s {
        Slot::Head => MSlot::Head,
        Slot::Chest => MSlot::Chest,
        Slot::Legs => MSlot::Legs,
        Slot::Feet => MSlot::Feet,
        Slot::Off => MSlot::OffHand,
        _ => MSlot::MainHand,
    }
}

/// The slot the mob takes `new` into, if any. `current(slot)` is the id held there (0: nothing); `preferred_tag` is the species' preferred
/// weapon tag (`getPreferredWeaponType`), `None` when it has none. The off hand is never replaced.
pub fn decide(new: u32, current: impl Fn(Slot) -> u32, preferred_tag: Option<&str>) -> Option<Slot> {
    let slot = slot_for(new);
    if beats(new, current(slot), slot, preferred_tag) {
        return Some(slot);
    }
    if !matches!(slot, Slot::Main | Slot::Off) && current(Slot::Main) == 0 {
        return Some(Slot::Main);
    }
    None
}

fn beats(new: u32, current: u32, slot: Slot, preferred_tag: Option<&str>) -> bool {
    if current == 0 {
        return true;
    }
    let preferred = preferred_tag.map(|t| (items::has_tag(t, new), items::has_tag(t, current)));
    can_replace_current_item(&facts(new), Some(&facts(current)), mob_slot(slot), preferred)
}
