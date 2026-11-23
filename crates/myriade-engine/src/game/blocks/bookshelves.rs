//! The enchanting power around an enchanting table (`EnchantmentMenu.slotsChanged`): the bookshelves.
use super::Game;
use myriade_engine::stations::bookshelf_offsets;
use horizon_world::registry::BlockStateId;

impl Game<'_> {
    /// The power around the table at `pos`: the sum, over `EnchantingTableBlock.BOOKSHELF_OFFSETS`
    /// whose block between it and the table is in `#minecraft:enchantment_power_transmitter`, of
    /// each block's `getEnchantPowerBonus`, truncated as NeoForge's `(int) power`. The offers cap
    /// it at 15.
    pub(super) fn count_bookshelves(&self, pos: [i32; 3]) -> i32 {
        let power: f32 = bookshelf_offsets()
            .into_iter()
            .filter(|o| {
                self.enchant_transmits(self.world.block(
                    pos[0] + o[0] / 2,
                    pos[1] + o[1],
                    pos[2] + o[2] / 2,
                ))
            })
            .map(|o| {
                let state = self.world.block(pos[0] + o[0], pos[1] + o[1], pos[2] + o[2]);
                self.enchant_power(state)
            })
            .sum();
        // A negative sum would make vanilla's `nextInt(bookcases + 1)` throw.
        (power as i32).max(0)
    }

    fn enchant_transmits(&self, state: BlockStateId) -> bool {
        self.items()
            .block_tags
            .get("minecraft:enchantment_power_transmitter")
            .is_some_and(|tag| tag.contains(&*self.res.facts().get(state).name))
    }

    /// Enchant power of the block in `state`: the provider-tag default.
    pub(in crate::game) fn enchant_power(&self, state: BlockStateId) -> f32 {
        self.items()
            .default_enchant_power(&self.res.facts().get(state).name)
    }
}
