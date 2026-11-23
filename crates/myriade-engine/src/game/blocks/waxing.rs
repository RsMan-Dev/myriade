//! `HoneycombItem.getWaxed` for the player's and the dispenser's honeycomb, and the axe's scrape
//! and wax-off from the world's NeoForge block maps.
use super::Game;
use horizon_world::registry::BlockStateId;

impl Game<'_> {
    /// The waxed twin of `state`, its properties kept: the world's
    /// `neoforge:waxables` value first (`DataMapHooks.getBlockWaxed`), else
    /// vanilla's `WAXABLES`, read from the axe's wax-off transforms. None where
    /// the block doesn't wax.
    pub(in crate::game) fn honeycomb_waxed(&self, state: BlockStateId) -> Option<BlockStateId> {
        if let Some(waxed) = self.res.behaviours().mapped_waxed(state) {
            return Some(waxed);
        }
        let registry = self.pack.assets.registry;
        let waxed = self
            .res
            .transformers()
            .waxed(registry, registry.state(state)?)?;
        registry.resolve(&waxed).ok()
    }

    /// The axe's scrape or wax-off of `block` from the world's NeoForge block maps, asked
    /// before the axe's transform table: the derived `neoforge:oxidizables` inverse
    /// (`DataMapHooks.getPreviousOxidizedStage`), then the `neoforge:waxables` inverse
    /// (`getBlockUnwaxed`). None for another tool or a block the maps leave out.
    pub(in crate::game) fn axe_mapped(
        &self,
        transformer: &str,
        block: BlockStateId,
    ) -> Option<(BlockStateId, myriade_types::ToolAbility)> {
        if transformer != "minecraft:axe" {
            return None;
        }
        let behaviours = self.res.behaviours();
        let scraped = behaviours.mapped_scraped(block);
        scraped
            .map(|state| (state, myriade_types::ToolAbility::AxeScrape))
            .or_else(|| {
                let unwaxed = behaviours.mapped_unwaxed(block)?;
                Some((unwaxed, myriade_types::ToolAbility::AxeWaxOff))
            })
    }

    /// The use a tool ability makes outside the transform table: `new`, the ability's sound
    /// and particle, no loot, one point of damage and no transform type.
    pub(in crate::game) fn tool_ability_result(
        new: BlockStateId,
        ability: myriade_types::ToolAbility,
    ) -> (BlockStateId, String, Option<String>, u32, String, String) {
        use myriade_types::ToolAbility::*;
        let (sound, particle) = match ability {
            AxeStrip => ("minecraft:item.axe.strip", "none"),
            AxeScrape => ("minecraft:item.axe.scrape", "scrape"),
            AxeWaxOff => ("minecraft:item.axe.wax_off", "wax_off"),
            ShovelFlatten => ("minecraft:item.shovel.flatten", "none"),
            HoeTill => ("minecraft:item.hoe.till", "none"),
            ShovelDouse => unreachable!(),
            _ => unreachable!("ToolAbility: unhandled variant"),
        };
        (
            new,
            sound.to_owned(),
            None,
            1,
            particle.to_owned(),
            String::new(),
        )
    }
}
