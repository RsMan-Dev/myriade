//! `Level.tickBlockEntities` for the block entities ticked together here
//! (hoppers, crafters, jukeboxes, beacons, creaking hearts, spawners, trial
//! spawners, vaults, beehives, furnaces and potent sulfur): only those whose state has a
//! ticker, in vanilla's list order (`myriade_engine::block_entities`), in
//! chunks that tick blocks. Sculk, brewing stands and pistons tick in their
//! own passes.
use super::Game;
use myriade_engine::{
    Result,
    block_entities::{Pos, Ticking},
    block_entity_anim::Ticker,
    containers::{BlockEntity, FurnaceKind, HOPPER_SLOTS},
    furnace::FurnaceCooking,
};
use horizon_core::position::BlockPos;
use horizon_gameplay::block_entity_ticks::{block_ticking_at, ticker_name};
use horizon_world::registry::BlockStateId;

/// What `block_entities.tick` is visiting: the kinds its dispatch ticks, read once per entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TickedKind {
    /// `HopperBlockEntity` (its transfer state is `Game::hoppers[pos]`).
    Hopper,
    /// `CrafterBlockEntity`.
    Crafter,
    /// `JukeboxBlockEntity`, while it holds a record.
    Jukebox,
    /// `BeaconBlockEntity`.
    Beacon,
    /// `CreakingHeartBlockEntity`.
    CreakingHeart,
    /// `SpawnerBlockEntity`.
    Spawner,
    /// `TrialSpawnerBlockEntity`.
    TrialSpawner,
    /// `VaultBlockEntity`.
    Vault,
    /// `BeehiveBlockEntity` (beehives and bee nests).
    Beehive,
    /// `AbstractFurnaceBlockEntity` (furnaces, blast furnaces, smokers).
    Furnace,
    /// `BrewingStandBlockEntity`.
    BrewingStand,
    /// `DaylightDetectorBlockEntity`.
    DaylightDetector,
    /// `TheEndGatewayBlockEntity`.
    EndGateway,
    /// `CampfireBlockEntity`.
    Campfire,
    /// A mod's ticking block entity.
    /// `PotentSulfurBlockEntity`.
    PotentSulfur,
}

impl TickedKind {
    /// What `block_entities.tick` ticks `entity` as; `None` for a block entity it does not tick.
    #[inline(always)]
    fn of(entity: &BlockEntity) -> Option<Self> {
        Some(match entity {
            BlockEntity::Chest(items) if items.len() == HOPPER_SLOTS => Self::Hopper,
            BlockEntity::Crafter(_) => Self::Crafter,
            BlockEntity::Jukebox(_) => Self::Jukebox,
            BlockEntity::Beacon(_) => Self::Beacon,
            BlockEntity::CreakingHeart(_) => Self::CreakingHeart,
            BlockEntity::Spawner(_) => Self::Spawner,
            BlockEntity::TrialSpawner(_) => Self::TrialSpawner,
            BlockEntity::Vault(_) => Self::Vault,
            BlockEntity::Beehive(_) => Self::Beehive,
            BlockEntity::Furnace(_) => Self::Furnace,
            BlockEntity::Brewing(_) => Self::BrewingStand,
            BlockEntity::DaylightDetector => Self::DaylightDetector,
            BlockEntity::EndGateway(_) => Self::EndGateway,
            BlockEntity::Campfire(_) => Self::Campfire,
            BlockEntity::PotentSulfur(_) => Self::PotentSulfur,
            _ => return None,
        })
    }
}

type PassScratch = (
    Option<crate::game::machines::ItemColumns>,
    Option<crate::game::potent_sulfur::PassMobs>,
    Vec<Pos>,
    Vec<Pos>,
);
/// Puts the enclosing pass's scratch back when the pass inside it ends, however it ends:
/// a caller may catch a nested pass's panic and let the enclosing pass go on.
struct Enclosing<'g, 'a> {
    game: &'g mut Game<'a>,
    scratch: Option<PassScratch>,
}
impl Drop for Enclosing<'_, '_> {
    fn drop(&mut self) {
        // A pass that ended early left its own; a pass in progress is known by them.
        self.game.item_columns = None;
        self.game.pass_mobs = None;
        if let Some(scratch) = self.scratch.take() {
            (
                self.game.item_columns,
                self.game.pass_mobs,
                self.game.spawner_tick_positions,
                self.game.trial_spawner_tick_positions,
            ) = scratch;
        }
    }
}

impl Game<'_> {
    /// One pass over the block entities.
    ///
    /// A run from inside a pass (a block entity that ticks the block entities again) is a whole pass of
    /// its own, the work after the loop included. It sets the enclosing pass's scratch (the item
    /// and mob caches, the spawners ticked so far) aside and puts it back when it ends, so the
    /// enclosing pass goes on with its own.
    pub(super) fn tick_block_entities(&mut self) -> Result<()> {
        // Only a pass in progress holds item columns: every pass drops them as it ends.
        let scratch = self.item_columns.is_some().then(|| {
            (
                self.item_columns.take(),
                self.pass_mobs.take(),
                std::mem::take(&mut self.spawner_tick_positions),
                std::mem::take(&mut self.trial_spawner_tick_positions),
            )
        });
        // The pass below is written on `this`, the game the guard lends back.
        let enclosing = Enclosing {
            game: self,
            scratch,
        };
        let this = &mut *enclosing.game;
        this.tick_craft_table();
        this.tick_container_reach();
        this.tick_merchant_reach();
        this.load_entity_types();
        if !this.world.block_entities.has_states() {
            let states = this.ticking_states();
            this.world.block_entities.set_states(states);
        }
        // Only lingering cooldowns are dropped; a hopper that ticks checks
        // its own block. Once a second is soon enough for the rest.
        if this.world.game_time % 20 == 0 {
            this.forget_gone_hoppers();
        }
        let ticking = this.block_ticking_area();
        this.spawner_tick_positions.clear();
        this.trial_spawner_tick_positions.clear();
        this.item_columns = Some(Default::default());
        this.pass_mobs = Some(Default::default());
        let tickers = this.world.block_entity_tickers();
        let mut last_chunk = (i32::MIN, i32::MIN);
        let mut last_chunk_ticking = false;
        for ticker in tickers.iter().copied() {
            let pos = ticker.0;
            let chunk = (pos.0 >> 4, pos.2 >> 4);
            let chunk_ticking = if chunk == last_chunk {
                last_chunk_ticking
            } else {
                last_chunk = chunk;
                last_chunk_ticking = ticking(pos.0, pos.2);
                last_chunk_ticking
            };
            if !chunk_ticking {
                continue;
            }
            // Ticks before this one can remove it (`TickingBlockEntity.isRemoved`).
            if this.world.block_entities.is_asleep(&pos)
                || !this.world.block_entities.is_live(ticker)
                || !this.border.contains(f64::from(pos.0), f64::from(pos.2))
            {
                continue;
            }
            let Some(kind) = this.world.block_entities.get(&pos).and_then(TickedKind::of) else {
                continue;
            };
            horizon_threads::work::set_chunk(chunk.0, chunk.1);
            horizon_threads::work::set_entity(horizon_threads::work::entity_id(
                this.world.block_entities.get(&pos).map_or("", ticker_name),
            ));
            match kind {
                TickedKind::Hopper => this.tick_hopper(pos),
                TickedKind::Crafter => this.tick_crafter(pos),
                TickedKind::Jukebox => this.tick_jukebox(pos),
                TickedKind::Beacon => this.tick_beacon_at(pos),
                TickedKind::CreakingHeart => this.tick_creaking_heart(pos)?,
                TickedKind::Spawner => {
                    this.tick_spawner(pos)?;
                    this.spawner_tick_positions.push(pos);
                }
                TickedKind::TrialSpawner => {
                    this.tick_trial_spawner(pos)?;
                    this.trial_spawner_tick_positions.push(pos);
                }
                TickedKind::Vault => this.tick_vault(pos)?,
                TickedKind::Beehive => this.tick_beehive(pos),
                TickedKind::Furnace => this.tick_furnace(pos)?,
                TickedKind::BrewingStand => this.tick_brewing_stand(pos)?,
                TickedKind::DaylightDetector => this.tick_daylight_detector(pos),
                TickedKind::EndGateway => this.tick_end_gateway(pos),
                TickedKind::Campfire => this.tick_campfire(pos)?,
                TickedKind::PotentSulfur => {
                    this.tick_potent_sulfur(pos)?;
                    // A window that simulates the world itself is its own
                    // client; a server's clients tick it on their own.
                }
            }
        }
        horizon_threads::work::clear_work();
        // Items move between passes: the columns hold for this one only.
        this.item_columns = None;
        this.pass_mobs = None;
        this.scan_for_hearts();
        // `ShulkerBoxBlockEntity.tick`: the lids of the boxes this level
        // animates. Their grown collision shape reaches the collision queries
        // through `World::shulkers`, and an opening lid pushes what it runs
        // into (`moveCollidedEntities`), as a moving piston does.
        let pushes = this.tick_shulkers();
        this.sync_shulkers();
        this.push_shulker_entities(pushes);
        Ok(())
    }
    /// `ShulkerBoxBlockEntity.tick` for the boxes in block-ticking chunks, in
    /// position order: the lid's new progress, and the swept box of an opening
    /// lid for `moveCollidedEntities`. A box whose block is gone is dropped.
    /// The boxes are those the level animates: a box opens (`start_open` and
    /// block event 1) before it has one, exactly as vanilla's block entity is
    /// there from the start.
    fn tick_shulkers(&mut self) -> Vec<super::piston_push::ShulkerPush> {
        let ticking = self.block_ticking_area();
        let mut pushes = Vec::new();
        let mut gone = Vec::new();
        let positions: Vec<[i32; 3]> = self.be_anims.shulkers.keys().copied().collect();
        for pos in positions {
            let state = self.world.block(pos[0], pos[1], pos[2]);
            let Some(Ticker::Shulker { facing }) = self.be_anims.ticker(state.0) else {
                gone.push(pos);
                continue;
            };
            if !ticking(pos[0], pos[2])
                || !self.border.contains(f64::from(pos[0]), f64::from(pos[2]))
            {
                continue;
            }
            let tick = self
                .be_anims
                .shulkers
                .get_mut(&pos)
                .map(|shulker| shulker.tick(pos, facing));
            let Some(tick) = tick else { continue };
            // `ShulkerBoxBlockEntity.doNeighborUpdates` at the start and the
            // end of an opening or closing lid: the box's collision shape
            // moved, so the neighbours' shapes and their redstone updates run.
            for _ in 0..tick.neighbor_updates {
                let state = self.world.block(pos[0], pos[1], pos[2]);
                let at = BlockPos::new(pos[0], pos[1], pos[2]);
                self.with_level(|b, level, ticks| {
                    b.shulker_neighbor_updates(level, ticks, at, state)
                });
            }
            if let Some(push) = tick.push {
                pushes.push((pos, facing, push));
            }
        }
        for pos in gone {
            self.be_anims.shulkers.remove(&pos);
        }
        pushes
    }
    /// Copies the shulker box lids into the world, where the collision queries
    /// read them (`SceneCollisions::shulker_collision_state`), as
    /// `sync_moving_pistons` does for pistons.
    pub(super) fn sync_shulkers(&mut self) {
        self.world.shulkers.clear();
        self.world
            .shulkers
            .extend(self.be_anims.shulkers.iter().map(|(p, s)| (*p, s.clone())));
    }
    /// `BlockState.getTicker` of every state, for the block entities ticked
    /// here. A creaking heart keeps its slot while uprooted (see
    /// `tick_creaking_heart`).
    pub(in crate::game) fn ticking_states(&self) -> Vec<Ticking> {
        let registry = self.pack.assets.registry;
        (0..registry.len())
            .map(|i| {
                let state = BlockStateId(i as u32);
                let name: &str = &self.res.facts().get(state).name;
                let factory = myriade_engine::block_entity_types::factory_block(name);
                match factory {
                    "minecraft:hopper"
                    | "minecraft:crafter"
                    | "minecraft:beacon"
                    | "minecraft:creaking_heart"
                    | "minecraft:spawner"
                    | "minecraft:trial_spawner"
                    | "minecraft:vault"
                    | "minecraft:beehive"
                    | "minecraft:bee_nest"
                    | "minecraft:brewing_stand"
                    | "minecraft:daylight_detector"
                    | "minecraft:end_gateway"
                    // Lit, cookTick; unlit, cooldownTick.
                    | "minecraft:campfire"
                    | "minecraft:soul_campfire" => Ticking::Yes,
                    // Every state but dry has a ticker.
                    "minecraft:potent_sulfur" => {
                        let dry = registry
                            .state(state)
                            .and_then(|s| s.properties.get("potent_sulfur_state"))
                            .is_some_and(|v| v == "dry");
                        if dry { Ticking::Idle } else { Ticking::Yes }
                    }
                    "minecraft:jukebox" => {
                        let record = registry
                            .state(state)
                            .and_then(|s| s.properties.get("has_record"))
                            .is_some_and(|v| v == "true");
                        if record { Ticking::Yes } else { Ticking::Idle }
                    }
                    n if FurnaceKind::from_block(n).is_some() => Ticking::Yes,
                    _ => Ticking::No,
                }
            })
            .collect()
    }
    /// `LevelChunk.isTicking` (block coordinates): a block-ticking chunk from
    /// every player's tickets (`update_chunk_tickets`), its entities loaded
    /// (`ServerLevel.areEntitiesLoaded`).
    pub(super) fn block_ticking_area(&self) -> impl Fn(i32, i32) -> bool + use<> {
        let levels = self.tickets.levels();
        let pending = self.world.entities_pending.clone();
        move |x, z| block_ticking_at(&levels, &pending, x, z)
    }
    /// Single-position read without copying the pending-entity set.
    pub(super) fn block_ticking_at(&self, x: i32, z: i32) -> bool {
        block_ticking_at(&self.tickets.levels(), &self.world.entities_pending, x, z)
    }
    /// `DaylightDetectorBlock.tickEntity` of the detector at `pos`.
    fn tick_daylight_detector(&mut self, pos: Pos) {
        // It updates every 20 game ticks (the behaviour checks this too).
        if self.world.game_time % 20 != 0 {
            return;
        }
        let at = BlockPos::new(pos.0, pos.1, pos.2);
        self.with_level(|b, level, ticks| b.tick_daylight_detector(level, ticks, at));
        let _ = self.process_level_events();
    }
    /// `AbstractFurnaceBlockEntity.serverTick` of the furnace at `pos`, and
    /// its block's `lit` following it.
    pub(super) fn tick_furnace(&mut self, pos: Pos) -> Result<()> {
        // What the hooks need from before the tick, without cloning stacks: the
        // fuel's id only while the furnace is out (it can light this tick), the
        // input's id (a last item is gone after it smelts) and the result count.
        let before = match self.world.block_entities.get(&pos) {
            Some(BlockEntity::Furnace(f)) => (
                f.cooking_timer,
                (f.lit_remaining == 0)
                    .then(|| f.items[1].as_ref().map(|s| s.item.clone()))
                    .flatten(),
                f.items[0].as_ref().map(|s| s.item.clone()),
                f.items[2].as_ref().map_or(0, |s| s.count),
                f.lit_remaining,
            ),
            _ => return Ok(()),
        };
        let mut drops = Vec::new();
        let (lit, smelted, fueled) = {
            let Some(BlockEntity::Furnace(furnace)) = self.world.block_entities.get_mut(&pos)
            else {
                return Ok(());
            };
            let items = world_items!(self);
            let lit = furnace.tick(
                &self.loop_context,
                world_recipes!(self),
                items,
                self.res.providers(),
                &mut drops,
                &|s| crate::crafting::CraftingRemainder::crafting_remainder(items, s),
            );
            let Some(BlockEntity::Furnace(f)) = self.world.block_entities.get(&pos) else {
                return Ok(());
            };
            let smelted = f.items[2]
                .as_ref()
                .filter(|after| after.count > before.3)
                .map(|after| (before.2.clone(), after.item.clone()));
            // The tick takes one off `lit_remaining`; only consuming fuel
            // sets it higher (a lava bucket leaves a bucket of the same count).
            let fueled = f.lit_remaining > before.4.saturating_sub(1);
            (lit, smelted, fueled)
        };
        // `AbstractFurnaceBlockEntity.serverTick`: `burn` and consuming fuel
        // set the changed flag, so a completed smelt or a refuel marks the
        // chunk and tells comparators even though the block state stays LIT.
        if smelted.is_some() || fueled {
            self.container_changed(pos);
        }
        for stack in drops {
            self.campfire_drop(pos, stack);
        }
        let Some(lit) = lit else {
            if matches!(self.world.block_entities.get(&pos), Some(BlockEntity::Furnace(f)) if f.idle())
            {
                self.world.block_entities.sleep(pos);
            }
            return Ok(());
        };
        let state = self.world.block(pos.0, pos.1, pos.2);
        if let Some(mut s) = self.pack.assets.registry.state(state).cloned()
            && s.properties.contains_key("lit")
        {
            s.properties.insert("lit".into(), lit.to_string());
            if let Ok(id) = self.pack.assets.registry.resolve(&s) {
                self.level_set([pos.0, pos.1, pos.2], id)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod jukebox_stop_tests {
    use super::Game;
    use myriade_engine::items::ItemStack;

}

#[cfg(test)]
mod furnace_changed_tests {
    use crate::game::game_tests::tests::server_game;
    use myriade_engine::{
        containers::{BlockEntity, Furnace, FurnaceKind},
        items::ItemStack,
    };
    use std::collections::HashMap;

    /// `AbstractFurnaceBlockEntity.serverTick`'s changed flag: `burn` sets it,
    /// and the old furnace calls `setChanged` at the end of the tick. A smelt
    /// that completes while the furnace stays LIT changes the output and the
    /// input without changing the block state, so nothing else marks it.
    #[test]
    #[ignore = "daily"]
    fn a_finished_smelt_marks_the_chunk() {
        let mut game_resources = None;
        let mut game = server_game(&mut game_resources);
        let state = game
            .pack
            .assets
            .registry
            .default_state("minecraft:furnace")
            .unwrap();
        let pos = (8, 64, 8);
        game.world.set(pos.0, pos.1, pos.2, state).unwrap();
        game.world.block_entities.insert(
            pos,
            BlockEntity::Furnace(Furnace {
                kind: FurnaceKind::Furnace,
                items: [
                    Some(ItemStack::new("minecraft:raw_iron", 1)),
                    Some(ItemStack::new("minecraft:coal", 1)),
                    None,
                ],
                lit_remaining: 5,
                lit_total: 200,
                cooking_timer: 199,
                cooking_total: 200,
                speed: 1.0,
                recipes_used: HashMap::new(),
            }),
        );
        game.world.modified.clear();

        game.tick_furnace(pos).expect("the furnace ticks");

        let Some(BlockEntity::Furnace(f)) = game.world.block_entities.get(&pos) else {
            panic!("no furnace");
        };
        assert_eq!(
            f.items[2].as_ref().map(|s| &*s.item),
            Some("minecraft:iron_ingot"),
            "the smelt completed"
        );
        assert!(
            game.world.modified.contains(&(0, 0)),
            "a finished smelt left the chunk unmarked"
        );
    }

    #[test]
    #[ignore = "daily"]
    fn a_refuel_while_lit_marks_the_chunk() {
        // litTimeRemaining goes 1 -> 0 and the furnace takes new fuel in the
        // same tick: it stays LIT, and consuming the fuel sets changed.
        let mut game_resources = None;
        let mut game = server_game(&mut game_resources);
        let state = game
            .pack
            .assets
            .registry
            .default_state("minecraft:furnace")
            .unwrap();
        let pos = (8, 64, 8);
        game.world.set(pos.0, pos.1, pos.2, state).unwrap();
        game.world.block_entities.insert(
            pos,
            BlockEntity::Furnace(Furnace {
                kind: FurnaceKind::Furnace,
                items: [
                    Some(ItemStack::new("minecraft:raw_iron", 1)),
                    Some(ItemStack::new("minecraft:coal", 2)),
                    None,
                ],
                lit_remaining: 1,
                lit_total: 1600,
                cooking_timer: 10,
                cooking_total: 200,
                speed: 1.0,
                recipes_used: HashMap::new(),
            }),
        );
        game.world.modified.clear();

        game.tick_furnace(pos).expect("the furnace ticks");

        let Some(BlockEntity::Furnace(f)) = game.world.block_entities.get(&pos) else {
            panic!("no furnace");
        };
        assert_eq!(f.items[1].as_ref().map(|s| s.count), Some(1), "fuel used");
        assert!(f.items[2].is_none(), "nothing smelted");
        assert!(
            game.world.modified.contains(&(0, 0)),
            "a refuel left the chunk unmarked"
        );
    }
}
