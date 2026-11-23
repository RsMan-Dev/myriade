//! The attribute modifiers an item grants in an equipment slot, for the ECS level's `ItemModifiers` table: P applies them when a slot
//! changes (`equip_attrs`), this table says which rows a stack gives (its own component, the item's data, mods' hook, `modifier_applies`).
use super::super::Game;
use myriade_engine::items::ItemStack;
use horizon_ecs_proto::{attr_mod::{Modifier, Operation}, equip_attrs::ItemModifiersFn};

impl Game<'_> {
    /// The table of the item modifiers by stack and slot ordinal. It holds the item table, not `self`; none yet (the world's tables are
    /// staged later) grants nothing until the table is set again.
    pub(in crate::game) fn item_modifiers_fn(&self) -> ItemModifiersFn {
        let items = self.server_items.clone().or_else(|| self.world_tables.as_ref().map(|t| t.items.clone()));
        Box::new(move |stack: &ItemStack, slot: usize| {
            let (Some(items), Some(slot_name)) = (items.as_deref(), horizon_items::equipment::SLOT_NAMES.get(slot)) else {
                return Vec::new();
            };
            myriade_engine::item_attributes::attribute_modifiers(stack, items)
                .iter()
                .filter(|row| myriade_engine::living_attributes::modifier_applies(&row.slot, slot_name))
                .map(|row| {
                    let operation = match row.operation.as_str() {
                        "add_multiplied_base" => Operation::AddMultipliedBase,
                        "add_multiplied_total" => Operation::AddMultipliedTotal,
                        _ => Operation::AddValue,
                    };
                    let name = row.attribute.strip_prefix("minecraft:").unwrap_or(&row.attribute).to_owned();
                    (name, Modifier { id: row.id.clone(), amount: row.amount, operation, permanent: false })
                })
                .collect()
        })
    }
}
