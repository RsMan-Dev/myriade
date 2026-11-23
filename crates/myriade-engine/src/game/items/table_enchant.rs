//! The enchanting table's result (`EnchantmentMenu.clickMenuButton`).
use super::*;

impl Game<'_> {
    /// The stack the table's `row`, costing `cost`, makes of `input` at `seed`: vanilla's
    /// (`myriade_engine::stations::apply_enchantment`). `None` when the row gives nothing.
    pub(in crate::game) fn table_enchant(
        &self,
        seed: i32,
        row: usize,
        cost: i32,
        input: &ItemStack,
    ) -> Option<ItemStack> {
        myriade_engine::stations::apply_enchantment(
            &self.stations().enchantments,
            self.items(),
            seed,
            row,
            cost,
            input,
        )
    }
}
