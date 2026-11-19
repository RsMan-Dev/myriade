//! Item facts the equipment reads from Horizon's data, by the ids of `items.rs` (installed first): the durability of an item (`ItemInfo.max_damage`,
//! what `ItemStack.getMaxDamage` gives with no stack override) and the wear Unbreaking leaves (the `minecraft:item_damage` effects of the
//! enchantments on the piece, `EnchantmentHelper.processDurabilityChange`, read through `Enchantments::modify_value`, never copied here).
//! A piece with no durability (0) never wears, never breaks.
use crate::items::item_name;
use horizon_client_lib::items::{ItemStack, Items};
use horizon_core::random::LegacyRandom;
use horizon_items::enchant::{EffectContext, Enchantments};
use horizon_items::item_data::Levels;
use std::sync::OnceLock;

/// The durability of every item, by the id of `items.rs` (index 0 unused).
static DURABILITY: OnceLock<Vec<i32>> = OnceLock::new();

/// Reads the durability of the registry; the same order as `items::install`, so the ids agree. A second call changes nothing.
pub fn install(items: &Items) {
    DURABILITY.get_or_init(|| {
        let names = if items.registry_order.is_empty() { &items.order } else { &items.registry_order };
        std::iter::once(0)
            .chain(names.iter().map(|n| items.get(n).map_or(0, |info| info.max_damage as i32)))
            .collect()
    });
}

/// `ItemStack.getMaxDamage` of the item with no stack override; 0 for an item that is not damageable or an unknown id.
pub fn durability_of(id: u32) -> i32 {
    DURABILITY.get().and_then(|v| v.get(id as usize)).copied().unwrap_or(0)
}

/// The durability that counts when the piece is hurt (`doHurtEquipment` checks `isDamageableItem`): the same table, as a stack carries
/// no override here.
pub fn durability_when_hurt(id: u32) -> i32 {
    durability_of(id)
}

/// The damage an item of `id` (carrying `levels`) takes from `amount` points of wear: what Unbreaking and the item's effects leave.
/// Unchanged for an item without `minecraft:item_damage` effects.
pub fn unbreaking_loss(items: &Items, enchantments: &Enchantments, id: u32, levels: &Levels, amount: u32, random: &mut LegacyRandom) -> u32 {
    let Some(name) = item_name(id) else { return amount };
    if levels.is_empty() {
        return amount;
    }
    let mut stack = ItemStack::new(name, 1);
    stack.data_mut().enchantments = levels.clone();
    if !enchantments.has_component(&stack, "minecraft:item_damage") {
        return amount;
    }
    let ctx = EffectContext { tool: Some(name), items: Some(items), ..Default::default() };
    enchantments.modify_value(Some(&stack), "minecraft:item_damage", &ctx, random, amount as f32).max(0.0) as u32
}
