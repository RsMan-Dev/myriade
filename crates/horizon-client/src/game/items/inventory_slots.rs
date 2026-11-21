//! The server's inventory rules: the container a player has open, and the recipes a craft unlocks.
use crate::game::Game;
use horizon_client::containers::BlockEntity;
use horizon_client::items::ItemStack;

impl Game<'_> {
    pub(in crate::game) fn container_kind(&self) -> Option<&horizon_client::containers::BlockEntity> {
        self.container
            .and_then(|k| self.world.block_entities.get(&k))
    }

    pub(in crate::game) fn award_crafted(&mut self, crafts: &[(String, Vec<Option<ItemStack>>)]) {
        for (recipe, ingredients) in crafts {
            // The whole id: the advancements' `recipe_crafted` criteria name
            // `minecraft:x`, and the crafting copy names the recipe bare.
            let recipe = horizon_client::crafting::qualified(recipe);
            self.trigger_recipe_crafted(&recipe, ingredients);
        }
        let recipes: Vec<String> = crafts
            .iter()
            .map(|(r, _)| horizon_client::crafting::qualified(r))
            .collect();
        self.award_recipes(&recipes);
    }
}

impl Game<'_> {
    /// 300.. are the open container's slots.
    pub(in crate::game) fn slots(&self) -> Vec<usize> {
        if let Some(v) = self.station_slots().or_else(|| self.mount_slots()) {
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

    /// InventoryMenu / CraftingMenu slot layouts.
    pub(in crate::game) fn slot_position(&self, i: usize) -> (f32, f32) {
        if let Some(p) = self
            .station_slot_position(i)
            .or_else(|| {
                Self::stock_machine_slot(self.machine_slot_position(i), self.native_screen_shown())
            })
            .or_else(|| self.crafter_slot_position(i))
        {
            return p;
        }
        let table = self.craft_size == 3;
        // ContainerScreen (3 rows) shifts the player inventory down by one pixel.
        // ChestMenu: the inventory sits `18 * (rows - 3) + 1` lower than the
        // 3-row layout of the other menus.
        let shift = match (self.chest_rows(), self.native_screen_inventory_y()) {
            (_, Some(y)) => y - 84.0,
            (Some(rows), None) => 1.0 + (rows as f32 - 3.0) * 18.0,
            (None, None) => 0.0,
        };
        let inventory_x = self.native_screen_inventory_x().unwrap_or(8.0);
        match i {
            0..9 => (inventory_x + i as f32 * 18.0, 142.0 + shift),
            9..36 => (
                inventory_x + ((i - 9) % 9) as f32 * 18.0,
                84.0 + shift + ((i - 9) / 9) as f32 * 18.0,
            ),
            500..504 => (8.0, 8.0 + (i - 500) as f32 * 18.0),
            // InventoryMenu: the shield slot.
            504 => (77.0, 62.0),
            300..500 if let Some(p) = self.native_screen_slot(i - 300) => p,
            300..500 => match self.container_kind() {
                Some(BlockEntity::Furnace(_)) => {
                    [(56.0, 17.0), (56.0, 53.0), (116.0, 35.0)][(i - 300).min(2)]
                }
                _ => (
                    8.0 + ((i - 300) % 9) as f32 * 18.0,
                    18.0 + ((i - 300) / 9) as f32 * 18.0,
                ),
            },
            200 => {
                if table {
                    (124.0, 35.0)
                } else {
                    (154.0, 28.0)
                }
            }
            _ => {
                let g = i - 100;
                let (r, c) = (g / self.craft_size, g % self.craft_size);
                if table {
                    (30.0 + c as f32 * 18.0, 17.0 + r as f32 * 18.0)
                } else {
                    (98.0 + c as f32 * 18.0, 18.0 + r as f32 * 18.0)
                }
            }
        }
    }
}
