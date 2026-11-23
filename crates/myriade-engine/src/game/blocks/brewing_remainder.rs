//! Stack-aware remainder decisions made before borrowing the brewing stand for consumption.
use super::*;
use myriade_engine::items::Items;

pub(super) struct BrewingRemainder(Option<Option<ItemStack>>);

impl BrewingRemainder {
    pub(super) fn resolve(&self, items: &Items, stack: &ItemStack) -> Option<ItemStack> {
        self.0
            .clone()
            .unwrap_or_else(|| crate::crafting::CraftingRemainder::crafting_remainder(items, stack))
    }
}

impl Game<'_> {
    pub(super) fn brewing_fuel_remainder(&self, _pos: (i32, i32, i32)) -> BrewingRemainder {
        BrewingRemainder(None)
    }

    pub(super) fn brewing_ingredient_remainder(&self, pos: (i32, i32, i32)) -> Option<ItemStack> {
        match self.world.block_entities.get(&pos) {
            Some(BlockEntity::Brewing(b)) => {
                b.items[3].as_ref().and_then(|s| {
                    myriade_engine::crafting::CraftingRemainder::crafting_remainder(world_items!(self), s)
                })
            }
            _ => None,
        }
    }
}
