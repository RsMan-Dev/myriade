//! The enchantments a mob's new piece gets at spawn (`Mob.enchantSpawnedWeapon` and `enchantSpawnedArmor` of Horizon's `finalize.rs`, `enchant_slot`
//! and `enchant_by_cost`), and the drowned's trident (`Drowned` in `populate`). The enchantment choice is Horizon's `Enchantments::select`
//! on the pool `#minecraft:on_mob_spawn_equipment`; nothing about the enchantments is written here.
use crate::rules::equipment_enchant as rule;
use myriade_engine_lib::items::{ItemStack, Items};
use horizon_core::random::RandomSource;
use horizon_items::enchant::Enchantments;
use horizon_items::item_data::Levels;

/// The chance that the main hand is enchanted, before the difficulty multiplier; the armour pieces use 0.5.
pub const MAIN_CHANCE: f32 = 0.25;
pub const ARMOR_CHANCE: f32 = 0.5;

/// Whether a piece is enchanted, then the enchantments it gets: `float >= chance * special` keeps it bare, else the cost is drawn from
/// `5 to 5 + 17 * special` and `select` picks what the cost buys. `name` is the item the piece is (no enchantment yet). The caller skips
/// an empty slot first: Horizon draws nothing for it.
pub fn enchant_slot<R: RandomSource>(
    items: &Items,
    enchantments: &Enchantments,
    name: &str,
    special: f32,
    chance: f32,
    random: &mut R,
) -> Levels {
    if random.next_float() >= chance * special {
        return Vec::new();
    }
    let cost = random.next_int_between_inclusive(5, rule::cost_top(special)).unwrap_or(5);
    let pool = enchantments.tag("minecraft:on_mob_spawn_equipment");
    enchantments.select(items, random, &ItemStack::new(name, 1), cost, pool)
}

/// The drowned's main hand at spawn: `float > 0.9`, then a trident in 10 of 16, else a fishing rod; `None` when the hand stays empty.
pub fn drowned_weapon<R: RandomSource>(random: &mut R) -> Option<&'static str> {
    rule::drowned_weapon(random, crate::species::lists::TRIDENT)
}
