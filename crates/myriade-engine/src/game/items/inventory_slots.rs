//! The server's inventory rules: the container a player has open, and the recipes a craft unlocks.
use crate::game::Game;
use myriade_engine::containers::BlockEntity;
use myriade_engine::items::ItemStack;

impl Game<'_> {
    pub(in crate::game) fn container_kind(&self) -> Option<&myriade_engine::containers::BlockEntity> {
        self.container
            .and_then(|k| self.world.block_entities.get(&k))
    }

    pub(in crate::game) fn award_crafted(&mut self, crafts: &[(String, Vec<Option<ItemStack>>)]) {
        for (recipe, ingredients) in crafts {
            // The whole id: the advancements' `recipe_crafted` criteria name
            // `minecraft:x`, and the crafting copy names the recipe bare.
            let recipe = myriade_engine::crafting::qualified(recipe);
            self.trigger_recipe_crafted(&recipe, ingredients);
        }
        let recipes: Vec<String> = crafts
            .iter()
            .map(|(r, _)| myriade_engine::crafting::qualified(r))
            .collect();
        self.award_recipes(&recipes);
    }
}

impl Game<'_> {
    /// 300.. are the open container's slots.
    pub(in crate::game) fn slots(&self) -> Vec<usize> {
        if let Some(v) = self.station_slots() {
            return v;
        }
        let mut v: Vec<usize> = (0..36).collect();
        if let Some(shown) = self.native_screen_slots() {
            v.extend(shown);
            return v;
        }
        match self.container_kind() {
            Some(BlockEntity::Chest(_)) => {
                v.extend((0..self.open_container_size()).map(|i| 300 + i))
            }
            Some(BlockEntity::Furnace(_)) => v.extend(300..303),
            Some(BlockEntity::Brewing(_)) => v.extend(300..305),
            Some(BlockEntity::Crafter(_)) => v.extend(300..310),
            Some(
                BlockEntity::Sign(_)
                | BlockEntity::Jukebox(_)
                | BlockEntity::Beacon(_)
                | BlockEntity::Lectern(_)
                | BlockEntity::Sculk(_)
                | BlockEntity::Banner(_)
                | BlockEntity::CopperGolemStatue(_)
                | BlockEntity::CreakingHeart(_)
                | BlockEntity::Spawner(_)
                | BlockEntity::TrialSpawner(_)
                | BlockEntity::Vault(_)
                | BlockEntity::Beehive(_)
                | BlockEntity::CommandBlock(_)
                | BlockEntity::EndGateway(_)
                | BlockEntity::Skull(_)
                | BlockEntity::DaylightDetector
                | BlockEntity::Bell
                | BlockEntity::Conduit(_)
                | BlockEntity::EndPortal
                | BlockEntity::EnchantingTable
                | BlockEntity::Campfire(_)
                | BlockEntity::ChiseledBookshelf(_)
                | BlockEntity::Brushable(_)
                | BlockEntity::Shelf(_)
                | BlockEntity::Mod(_)
                | BlockEntity::PotentSulfur(_)
                | BlockEntity::StructureBlock(_)
                | BlockEntity::JigsawBlock(_)
                | BlockEntity::TestBlock(_)
                | BlockEntity::TestInstanceBlock(_),
            ) => {}
            None => {
                v.extend((0..self.craft_size * self.craft_size).map(|i| 100 + i));
                v.push(200);
                if self.craft_size == 2 {
                    v.extend(500..505);
                }
            }
        }
        v
    }

    pub(in crate::game) fn slot_mut(&mut self, i: usize) -> &mut Option<ItemStack> {
        if i >= crate::game::mount_inventory::SADDLE {
            return self.mount_slot_mut(i);
        }
        if i >= crate::game::stations::INPUT {
            return self.station_slot_mut(i);
        }
        if (500..504).contains(&i) {
            return &mut self.armor[i - 500];
        }
        if i == 504 {
            return &mut self.offhand;
        }
        if i >= 300 {
            return self.container_slot_mut(i - 300);
        }
        if (100..109).contains(&i) {
            &mut self.craft_grid[i - 100]
        } else {
            &mut self.inventory[i]
        }
    }
}
