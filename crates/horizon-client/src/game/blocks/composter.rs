//! `ComposterBlock`: compostable items go in by hand (`useItemOn`) or
//! through its input container (hoppers, droppers, crafters above), and the
//! fill feedback of level event 1500 (`handleFill`). Emptying a full
//! composter by hand is `useWithoutItem` in horizon-world.
use super::{Game, commands::compute::ComputeContext};
use horizon_client::{
    items::ItemStack,
    mobs::villager,
};
use horizon_core::position::BlockPos;
use horizon_world::registry::BlockStateId;

/// Whether a block type provides the composter's state-backed container ports.
pub(super) fn is_composter(kind: &str) -> bool {
    kind == "minecraft:composter"
}

/// State-backed container feedback, published only by the outermost successful transaction.
pub(super) enum Effects {
    Insert {
        pos: [i32; 3],
        before: BlockStateId,
        after: BlockStateId,
    },
    Empty {
        pos: [i32; 3],
        after: BlockStateId,
    },
}

/// Temporarily lends the provider the composter's random owner. Even unwinding restores
/// Level.random before the transaction guard rolls the local owner back.
struct ScopedRandom<'a, 'w> {
    game: &'a mut Game<'w>,
    pos: [i32; 3],
    world_random: Option<horizon_core::random::LegacyRandom>,
}

impl Drop for ScopedRandom<'_, '_> {
    fn drop(&mut self) {
        let mut random = self.world_random.take().unwrap();
        std::mem::swap(&mut *self.game.world.random, &mut random);
        self.game.put_composter_random(self.pos, random);
    }
}

impl Game<'_> {
    /// The composter's `level` at `pos`, None when it isn't a composter.
    pub(super) fn composter_level(&self, pos: [i32; 3]) -> Option<i32> {
        let state = self.world.block(pos[0], pos[1], pos[2]);
        let s = self.pack.assets.registry.state(state)?;
        if !is_composter(&s.name) {
            return None;
        }
        s.properties.get("level")?.parse().ok()
    }
    /// The item's `compostable` layers provider.
    fn compostable(&self, stack: &ItemStack) -> Option<serde_json::Value> {
        self.items().get(&stack.item)?.compostable.clone()
    }
    /// The shared `ComposterBlock.addLayer` block-interaction loot context and level random.
    fn composter_layers(&mut self, pos: [i32; 3], provider: &serde_json::Value) -> i32 {
        let context = ComputeContext {
            block: Some(pos),
            target: None,
        };
        self.provider_int(provider, &context, 0).unwrap_or(0)
    }

    /// ComposterWrapper's private transactional random, using the unchanged vanilla layers
    /// provider. Horizon initializes the owner from the current level stream once; it never
    /// advances or rewinds the regular level stream while evaluating this capability.
    fn composter_transactional_layers(
        &mut self,
        pos: [i32; 3],
        provider: &serde_json::Value,
    ) -> i32 {
        let mut random = self.take_composter_random(pos);
        std::mem::swap(&mut *self.world.random, &mut random);
        let scope = ScopedRandom {
            game: self,
            pos,
            world_random: Some(random),
        };
        scope.game.composter_layers(pos, provider)
    }
    /// `ComposterBlock.addLayer` and level event 1500 for immediate vanilla callers.
    fn composter_add(&mut self, pos: [i32; 3], layers: &serde_json::Value) -> bool {
        let layers = self.composter_layers(pos, layers);
        let before = self.world.block(pos[0], pos[1], pos[2]);
        let at = BlockPos::new(pos[0], pos[1], pos[2]);
        let after = self.with_level(|b, level, ticks| b.composter_insert(level, ticks, at, layers));
        self.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
        // The game event and level event 1500 happen here, before the caller goes on.
        let _ = self.process_level_events();
        after != before
    }
    /// `ComposterBlock.useItemOn` with the main-hand stack: true when it
    /// consumed the action (SUCCESS; a full composter falls through to
    /// `useWithoutItem`, which empties it).
    pub(super) fn use_composter(&mut self, pos: [i32; 3], held: Option<&ItemStack>) -> bool {
        let Some(level) = self.composter_level(pos) else {
            return false;
        };
        let Some(stack) = held else {
            return false;
        };
        let Some(layers) = self.compostable(stack) else {
            return false;
        };
        if level >= 8 {
            return false;
        }
        if level < 7 {
            self.composter_add(pos, &layers);
            self.native_compost_item(pos, &stack.item);
            self.award_stat("used", &stack.item, 1);
            self.consume_held();
        }
        // ServerPlayerGameMode.useItemOn: the action was consumed.
        self.trigger_block("minecraft:item_used_on_block", pos, stack);
        true
    }
    /// `HopperBlockEntity.addItem` into the composter's `InputContainer`
    /// (levels 0 to 6; one item, compostable, from above): the remainder.
    pub(super) fn composter_container_insert(
        &mut self,
        pos: [i32; 3],
        mut stack: ItemStack,
        from_above: bool,
    ) -> Option<ItemStack> {
        let Some(level) = self.composter_level(pos) else {
            return Some(stack);
        };
        if !from_above || level >= 7 {
            return Some(stack);
        }
        let Some(layers) = self.compostable(&stack) else {
            return Some(stack);
        };
        // InputContainer.getMaxStackSize is 1: one item goes in, and its
        // setChanged composts it at once.
        stack.count -= 1;
        self.composter_add(pos, &layers);
        (stack.count > 0).then_some(stack)
    }

    /// `InputContainer.setChanged` through an ITEMS port. Slot acceptance and the one-item
    /// limit are the shared container rules; only the state write and feedback are deferred.
    pub(super) fn composter_capability_insert(&mut self, pos: [i32; 3], stack: &ItemStack) -> bool {
        if self.composter_level(pos).is_none_or(|level| level >= 7) {
            return false;
        }
        let Some(provider) = self.compostable(stack) else {
            return false;
        };
        let before = self.world.block(pos[0], pos[1], pos[2]);
        let layers = self.composter_transactional_layers(pos, &provider);
        let after = self
            .res
            .behaviours()
            .composter_state_after_layers(before, layers);
        if after != before && !self.journal_set_block(pos, after) {
            return false;
        }
        self.after_commit(super::transactions::After::Composter(Effects::Insert {
            pos,
            before,
            after,
        }));
        true
    }

    /// `OutputContainer.setChanged` through an ITEMS port: no item entity or hand sound.
    pub(super) fn composter_capability_extract(
        &mut self,
        pos: [i32; 3],
        max: u32,
    ) -> Option<ItemStack> {
        if max == 0 || self.composter_level(pos) != Some(8) {
            return None;
        }
        let before = self.world.block(pos[0], pos[1], pos[2]);
        let after = self.res.behaviours().composter_state_after_extract(before);
        if !self.journal_set_block(pos, after) {
            return None;
        }
        self.after_commit(super::transactions::After::Composter(Effects::Empty {
            pos,
            after,
        }));
        Some(ItemStack::new("minecraft:bone_meal", 1))
    }

    pub(super) fn commit_composter_effects(&mut self, effects: Effects) {
        self.with_level(|b, level, ticks| match effects {
            Effects::Insert { pos, before, after } => {
                b.composter_insert_effects(
                    level,
                    ticks,
                    BlockPos::new(pos[0], pos[1], pos[2]),
                    before,
                    after,
                );
            }
            Effects::Empty { pos, after } => {
                b.composter_empty_effects(
                    level,
                    ticks,
                    BlockPos::new(pos[0], pos[1], pos[2]),
                    after,
                );
            }
        });
        let _ = self.process_level_events();
    }
    /// `villager_compost_items` for the ECS villager `id`: the same batch, its stacks read from the bridge (`ecs.inventory_stacks`)
    /// and taken one by one (`ecs.inventory_take`); the level side is shared with the client's copy.
    pub(super) fn villager_compost_items_ecs(&mut self, id: u32, pos: [i32; 3]) {
        let at = BlockPos::new(pos[0], pos[1], pos[2]);
        let Some(mut before) = self.composter_level(pos) else {
            return;
        };
        if before == 8 {
            self.with_level(|b, level, ticks| b.composter_extract(level, ticks, at));
            let _ = self.process_level_events();
            before = 0;
        }
        let mut remaining = 20;
        let maps = std::sync::Arc::clone(self.mob_map_snapshot());
        let stacks = self.ecs.inventory_stacks(id);
        'stacks: for stack in stacks.iter().rev() {
            if remaining <= 0 {
                break;
            }
            if !maps.villager_compostable(&stack.item) {
                continue;
            }
            let uses = (stack.count as i32 - 10).min(remaining).min(stack.count as i32);
            if uses <= 0 {
                continue;
            }
            remaining -= uses;
            for _ in 0..uses {
                self.villager_insert_item_ecs(id, stack, pos);
                if self.composter_level(pos) == Some(7) {
                    break 'stacks;
                }
            }
        }
        let after = self.composter_level(pos).unwrap_or(before);
        self.with_level(|b, level, ticks| {
            b.composter_fill_effects(level, ticks, at, after != before)
        });
        let _ = self.process_level_events();
    }
    /// `villager_insert_item` for the ECS villager `id`: one item of `stack` is taken from it through the bridge.
    fn villager_insert_item_ecs(&mut self, id: u32, stack: &ItemStack, pos: [i32; 3]) {
        if self.composter_level(pos).is_none_or(|level| level >= 7) {
            return;
        }
        let Some(layers) = self.compostable(stack) else {
            return;
        };
        let layers = self.composter_layers(pos, &layers);
        let at = BlockPos::new(pos[0], pos[1], pos[2]);
        self.with_level(|b, level, ticks| b.composter_layer(level, ticks, at, layers));
        self.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
        let _ = self.process_level_events();
        self.ecs.inventory_take(id, &stack.item, 1);
    }
}

#[cfg(test)]
#[path = "composter/capability_tests.rs"]
mod capability_tests;

impl Game<'_> {
    pub(super) fn composter_fill_event(&mut self, pos: BlockPos, success: bool) {
        let center = [
            f64::from(pos.x) + 0.5,
            f64::from(pos.y) + 0.5,
            f64::from(pos.z) + 0.5,
        ];
        let sound = if success {
            "minecraft:block.composter.fill_success"
        } else {
            "minecraft:block.composter.fill"
        };
        // `handleFill` is `playLocalSound`: the client that receives event 1500
        // plays it.
        self.audio.play(sound, Some(center), 1.0, 1.0);
    }
}
