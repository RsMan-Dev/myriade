//! `host.transaction` (arismoko/horizon#41 §7): NeoForge 26.3's `Transaction` and `SnapshotJournal`, kept by
//! the game at the storage level, so a mod's handlers hold no state and write no journal.
//!
//! A capability write applies in place and keeps its storage's first before-image per depth.
//! Game-held vanilla containers defer implicit changed/comparator effects to root commit;
//! nested commits hand their snapshots and waiting work to the parent. An abort restores
//! storage quietly and discards those commit effects. Raw mod-owned storage writes and
//! restores synthesize no notice. Explicit notifications a mod issues run immediately and
//! survive abort: the journal restores storage, not observer calls or scheduled comparator
//! ticks that already happened.
//!
//! Covered here: block entities (slots, saved compound, custom name, lock, loot roll, attachments), a hopper's cooldown and
//! cauldron and composter block states. The attachment map of every other holder (an entity, a player, a
//! chunk, the level) is kept on the first access to it through `attachment_map`. A block's tentative state is stored only (`World::set_stored`): the
//! one real write, with its remesh, relight and logs, is at the root commit. A jukebox is
//! told at the root commit from its record before and after, so a disc swapped in one
//! transaction changes the song. A player's inventory (slots, armour, off hand, cursor) is kept
//! whole on its first write, from wherever the player's state lives then: the local player's or
//! a guest's, swapped in by `native_as_player`. An item a write drops from a player waits for
//! the root commit too. An item entity's stack is kept on its first write; one emptied stays,
//! empty, until the root commit removes it. A guest's menu storage (ender chest, crafting grid,
//! a mod's chest menu) is kept whole on its first write through a menu slot; a crafting grid
//! changed by the transaction has its result made again at the root commit.
//! `LivingRef::set_health` keeps the player's synced float storage and unpublished update
//! events, or the mob's health and sync dirtiness. This does not journal other health,
//! damage, death or attribute operations.
//! An entity added inside one goes at an abort, one removed is only marked until the root
//! commit, and mods hear of joins and leaves at the root commit (`transactions/entities.rs`).
use super::{BlockEntity, BlockStateId, Game, machines::HopperState};
use myriade_engine::{items::ItemStack, world::BlockEntityExtras};
use horizon_mod_runtime::attachments::{Attachments, Holder};
use mod_api::{Transaction, TransactionLifecycle, TransactionSeal};
use std::collections::HashMap;

#[path = "transactions/decision.rs"]
pub(in crate::game) mod decision;
#[path = "transactions/entities.rs"]
mod entities;
pub(in crate::game) use entities::{Announcement, ArrivalRollback};
#[path = "transactions/scratch.rs"]
mod scratch;

type Pos = (i32, i32, i32);
type ComposterRandoms =
    HashMap<(mod_api::DimensionKey, (i32, i32)), HashMap<Pos, horizon_core::random::LegacyRandom>>;

/// The open transactions, innermost last, and which close is running, if any: what
/// `transaction_lifecycle` reads.
#[derive(Default)]
pub(super) struct OpenTransactions {
    levels: Vec<Level>,
    closing: Option<TransactionLifecycle>,
    /// The root commits telling their joins and leaves, outermost first: a listener's command in
    /// another level can commit a transaction there while this level's commit is told.
    committing: Vec<entities::Committing>,
    composter_epoch: Option<mod_api::WorldEpoch>,
    /// Each composter port owns its transactional random stream, independently of Level.random.
    composter_randoms: ComposterRandoms,
    #[cfg(test)]
    pub(in crate::game) trace: Option<std::sync::Arc<std::sync::Mutex<Vec<String>>>>,
}

impl std::ops::Deref for OpenTransactions {
    type Target = Vec<Level>;
    fn deref(&self) -> &Vec<Level> {
        &self.levels
    }
}

impl std::ops::DerefMut for OpenTransactions {
    fn deref_mut(&mut self) -> &mut Vec<Level> {
        &mut self.levels
    }
}

/// Puts the closing phase back when a close ends, however it ends.
struct ClosingPhase<'a, 'g> {
    game: &'a mut Game<'g>,
    outer: Option<TransactionLifecycle>,
}

impl Drop for ClosingPhase<'_, '_> {
    fn drop(&mut self) {
        self.game.transactions.closing = self.outer;
    }
}

/// One open transaction.
#[derive(Default)]
pub(super) struct Level {
    /// What names it to the mods that write in it (`Transaction`): unguessable in practice, so
    /// only the value the game handed out names it.
    token: u64,
    /// What ends it (`TransactionSeal`), drawn apart from `token`: only whoever opened it holds
    /// it, never a handler lent the `Transaction`.
    close_token: u64,
    dimension: Option<super::Dim>,
    /// Each block entity as it was before this transaction first wrote it.
    block_entities: HashMap<Pos, Kept>,
    /// The block entities this transaction removed (`journal_remove_block_entity`): an abort
    /// puts them back as `block_entities` keeps them.
    removed_block_entities: std::collections::HashSet<Pos>,
    /// Each item slot (block entity, slot) a slot-only write changed, as it was before this
    /// transaction first wrote it, for a block entity not kept whole in `block_entities`: a
    /// move through a container keeps the slot it touches, not the entity's whole saved data.
    slots: HashMap<(Pos, usize), KeptSlot>,
    /// Each block's state before this transaction first wrote it.
    blocks: HashMap<Pos, BlockStateId>,
    /// Composter-local random streams before this depth first evaluated their layers.
    composter_randoms: ComposterRandoms,
    /// Each quart cell before this transaction first wrote it.
    biomes: HashMap<Pos, u16>,
    /// Each player's inventory before this transaction first wrote it, by UUID.
    players: HashMap<u128, KeptPlayer>,
    /// Synced player health and its unpublished update events before set_health.
    player_health: HashMap<u128, KeptPlayerHealth>,
    /// Mob health and synchronization dirtiness before set_health.
    mob_health: HashMap<u32, KeptMobHealth>,
    /// Each entity's inventory (`entity_slots`) before this transaction first wrote it.
    entities: HashMap<u32, Vec<Option<ItemStack>>>,
    /// Each mob's or armor stand's equipment before this transaction first
    /// wrote it (arismoko/horizon#932), told apart for the way back.
    equipment: HashMap<u32, KeptEquipment>,
    /// Each item entity's stack before this transaction first wrote it.
    item_entities: HashMap<u32, ItemStack>,
    /// Each guest's menu storage before this transaction first wrote it, by connection.
    menus: HashMap<u32, KeptMenus>,
    /// Each entity's, player's, chunk's and the level's attachment map before this transaction
    /// first reached it (a block entity's is kept with it).
    attachments: HashMap<Holder, KeptAttachments>,
    /// Held once this transaction reaches an attachment map, until it closes: the save
    /// admission gate (`passes_settled`) refuses a save while a map may hold tentative
    /// values, which only the root commit makes saveable.
    save_gate: Option<myriade_engine::join_seq::visibility::PassLease>,
    /// Each chunk's unsaved mark and save queue entry before this transaction first reached an
    /// attachment map in it: an abort puts them back, so a chunk only a tentative write marked
    /// is clean again.
    chunk_marks: HashMap<(i32, i32), myriade_engine::world::ChunkMark>,
    /// What the root commit does, in order.
    after: Vec<After>,
    /// Internal decide/fire/apply state. No menu route uses this yet.
    actions: decision::Journal,
    action: Option<decision::ActiveAction>,
    /// The scratch stacks (`ItemPlace::Scratch`), live: kept by the outermost transaction only.
    scratch: HashMap<u64, scratch::Scratch>,
    /// Each scratch stack before this transaction first wrote it; none where it didn't exist.
    scratch_kept: HashMap<u64, Option<scratch::Scratch>>,
    /// The entities it added and removed, and the announcements it holds.
    lifetimes: entities::EntityJournal,
    /// Each attachment map that went with an entity to another level after this transaction
    /// first reached it, by the id it arrived under, as it was then.
    travelled: HashMap<u32, KeptAttachments>,
    /// The index rows of the entities that left this level, kept until the root closes.
    departed: Vec<entities::Departure>,
    /// The entities taken for a transfer whose arrival is not adopted yet, by source id, each
    /// with the slot its arrival reads an abort's way back from.
    in_flight: HashMap<u32, entities::ArrivalRollback>,
}

/// A slot's stack before a transaction first wrote it, and the generation of the block entity
/// it belongs to: an abort puts it back only into that same block entity, never into one that
/// replaced it.
struct KeptSlot {
    stack: Option<ItemStack>,
    generation: Option<u64>,
}

struct KeptAttachments {
    contents: Attachments,
    indexed: bool,
}

struct Kept {
    entity: Option<BlockEntity>,
    generation: Option<u64>,
    /// None where it had no map: an abort removes one a write made.
    attachments: Option<Attachments>,
    hopper: Option<HopperState>,
    /// The custom name Horizon keeps beside the model (`World::block_names`).
    name: Option<horizon_core::nbt::Tag>,
    /// The container lock and loot table roll kept beside the model.
    extras: BlockEntityExtras,
}

impl Kept {
    fn capture(
        world: &myriade_engine::world::World,
        hoppers: &HashMap<Pos, HopperState>,
        pos: Pos,
    ) -> Self {
        Self {
            entity: world.block_entities.get(&pos).cloned(),
            generation: world.block_entities.generation(&pos),
            attachments: world.block_entities.attachments(pos).cloned(),
            hopper: hoppers.get(&pos).copied(),
            name: world.block_names.get(&pos).cloned(),
            extras: world.block_entity_extras(pos),
        }
    }
}

struct KeptPlayerHealth {
    data: horizon_world::synced_floats::SyncedFloats,
    events_len: usize,
    dead: bool,
    guest: Option<(u32, bool, u32)>,
}

/// A mob's health, absorption and death bookkeeping as a transaction first saw them, read and written through the bridge.
struct KeptMobHealth {
    absorption: f32,
    health: f32,
    processed: bool,
}

#[derive(Clone)]
struct KeptPlayer {
    ender_items: Vec<Option<ItemStack>>,
    player_extra: Option<horizon_core::nbt::Tag>,
    inventory: Vec<Option<ItemStack>>,
    armor: [Option<ItemStack>; 4],
    offhand: Option<ItemStack>,
    equipment: horizon_items::equipment::Equipment,
    carried: Option<ItemStack>,
}

/// A mob's or an armor stand's equipment as a transaction first wrote it
/// (arismoko/horizon#932): the same eight slots, restored their own way.
#[derive(Clone)]
enum KeptEquipment {
    /// An ECS entity's slots, put back through the bridge.
    Ecs(Vec<Option<ItemStack>>),
    Stand(Vec<Option<ItemStack>>),
}

#[derive(Clone)]
struct KeptMenus {
    ender_items: Vec<Option<ItemStack>>,
    craft_grid: [Option<ItemStack>; 4],
    native_menu: Vec<Option<ItemStack>>,
}

/// What a write tells the world, at the root commit.
pub(super) enum After {
    /// Reserved raw bytes publish only when their connection incarnation is still live.
    Payload(super::payload_transport::Deferred),
    Cauldron(super::cauldron_interactions::Effects),
    Composter(super::composter::Effects),
    /// A mod's world pickup or placement of fluid, its block already set: what is left of it
    /// (`Game::commit_fluid_write`).
    FluidWrite(Box<super::world_fluids::WrittenFluid>),
    /// A bucket's sound and fluid game event (`level_play_fluid_effects`).
    FluidEffects(Box<super::world_fluids::FluidEffects>),
    /// `BlockEntity.setChanged`.
    Changed(Pos),
    /// A provider's answer changed here (`level_invalidate_capabilities`): the capability
    /// generation moves on.
    Capabilities(Pos),

    /// `ServerLevel.destroyBlockProgress`: breaker id, position, stage. Sent once, at the root commit.
    BreakProgress(i32, [i32; 3], i32),
    /// The block's new state, set with its updates (`Level.setBlock`, `UPDATE_ALL`).
    Block(Pos),
    /// Save, remesh and resend a changed biome column.
    Biome((i32, i32)),
    /// Deferred interaction effects, replayed in the same root-commit order.
    Effect(DeferredEffect),
    /// `Block.popResource`.
    Drop([i32; 3], ItemStack),
    /// `Player.drop(stack, false)` by the player with this UUID: what no longer fits.
    PlayerDrop(u128, ItemStack),
    GivePickup(u128),
    InventoryTrigger(u128),
    GiveRest(u128, ItemStack, Option<[i32; 4]>),
    PlayerEquip(u128, String, Option<ItemStack>),
    /// An item split off an item entity, dropped where it is (`Block.popResource`'s delay).
    Spawn([f64; 3], ItemStack),
    /// The item entity with this id was emptied: removed, unless it holds items again.
    Emptied(u32),
    /// `LivingEntity.onEquipItem`'s sound and equip/unequip game event, at the entity `who`
    /// is: what an equipment write through a transaction tells the world only at the root
    /// commit, so an abort stays quiet (arismoko/horizon#932).
    Equip {
        at: [f64; 3],
        sound: Option<String>,
        equips: bool,
        who: u32,
    },
}

/// Effects shared by transactional interaction contexts.
pub(super) enum DeferredEffect {
    CustomStat {
        player: u128,
        key: &'static str,
        amount: i32,
    },
    /// `Player.awardStat` of an item statistic (`used`, `crafted`...) for `item`.
    ItemStat {
        player: u128,
        stat: &'static str,
        item: std::sync::Arc<str>,
        amount: i32,
    },
    Sound {
        sound: &'static str,
        at: [f64; 3],
        volume: f32,
        pitch: f32,
    },
}

/// A fresh transaction token: a process-wide count through a randomly keyed 64-bit hash, so a
/// mod cannot guess, in practice, the token of a transaction it was not handed.
fn frame_token() -> u64 {
    use std::hash::BuildHasher;
    use std::sync::{OnceLock, atomic::AtomicU64, atomic::Ordering};
    static KEYS: OnceLock<std::hash::RandomState> = OnceLock::new();
    static COUNT: AtomicU64 = AtomicU64::new(0);
    KEYS.get_or_init(std::hash::RandomState::new)
        .hash_one(COUNT.fetch_add(1, Ordering::Relaxed))
}

impl Game<'_> {
    /// Actual transaction/restore events for storage-attempt regression traces.
    #[cfg(test)]
    pub(in crate::game) fn trace_storage_attempt(&self, entry: String) {
        if let Some(trace) = &self.transactions.trace {
            trace.lock().unwrap().push(entry);
        }
    }
    /// Opens a transaction, nested in the open one if any (`Transaction.open`), and gives back
    /// the seal that ends it.
    pub(in crate::game) fn transaction_open(&mut self) -> TransactionSeal {
        let (token, close) = self.open_storage_transaction();
        mod_api::__private::transaction_seal(token, close)
    }
    /// Opens a transaction and gives back its write and close tokens.
    fn open_storage_transaction(&mut self) -> (u64, u64) {
        if let Some(scope) = &mut self.block_entity_tick_items {
            scope.open_transaction();
        }
        let (token, close_token) = (frame_token(), frame_token());
        if self.transactions.is_empty() {
            self.world.block_entities.defer_retirement();
        }
        let lifetimes = self.entity_journal_open();
        self.transactions.push(Level {
            token,
            close_token,
            dimension: Some(self.dim),
            lifetimes,
            ..Level::default()
        });
        #[cfg(test)]
        self.trace_storage_attempt("open".into());
        if let Some(key) = &mut self.block_entity_tick_key {
            key.unbalanced = true;
        }
        (token, close_token)
    }
    /// Closes the transaction `seal` ends, first aborting every one opened inside it and left
    /// open, so a guard closing its own is never stranded behind a child a mod abandoned.
    /// Refused when `seal` names no open transaction: only whoever opened one ends it, never a
    /// handler lent its `Transaction` (a hopper's move lends the game's own).
    pub(in crate::game) fn transaction_close(&mut self, seal: &TransactionSeal, commit: bool) {
        let (token, close) = mod_api::__private::seal_tokens(seal);
        let Some(at) = self
            .transactions
            .iter()
            .position(|level| level.token == token && level.close_token == close)
        else {
            log::warn!("Horizon: refused to close a transaction the seal does not end");
            return;
        };
        if self.transactions.len() > at + 1 {
            log::warn!("Horizon: aborted a transaction left open inside the one being closed");
        }
        while self.transactions.len() > at + 1 {
            self.close_storage_transaction(false);
        }
        self.close_storage_transaction(commit);
    }
    /// Whether `tx` names an open transaction: a write given any other changes nothing, so none
    /// happens outside the journal.
    pub(in crate::game) fn transaction_holds(&self, tx: &Transaction) -> bool {
        let named = mod_api::__private::transaction_token(tx);
        self.transactions.iter().any(|level| level.token == named)
    }
    /// `transaction_lifecycle`: the close running, else whether one is open.
    pub(in crate::game) fn transaction_lifecycle(&self) -> TransactionLifecycle {
        match self.transactions.closing {
            Some(closing) => closing,
            None if self.transactions.is_empty() => TransactionLifecycle::None,
            None => TransactionLifecycle::Open,
        }
    }
    /// `transaction_current`: the innermost open transaction's write token, never its seal.
    pub(in crate::game) fn transaction_current(&self) -> Option<Transaction> {
        let level = self.transactions.last()?;
        Some(mod_api::__private::transaction_of(level.token))
    }
    /// `transaction_depth`: how deeply `tx` is nested, 0 for the outermost.
    pub(in crate::game) fn transaction_depth(&self, tx: &Transaction) -> Option<u32> {
        let named = mod_api::__private::transaction_token(tx);
        let at = self
            .transactions
            .iter()
            .position(|level| level.token == named)?;
        u32::try_from(at).ok()
    }
    /// Closes the innermost transaction: `commit` hands its changes to the one around it, or
    /// at the root makes them final and tells the world; otherwise every storage it wrote goes
    /// back to how it was. Nothing happens with none open.
    fn close_storage_transaction(&mut self, commit: bool) {
        let Some(level) = self.transactions.pop() else {
            return;
        };
        let closing = if self.transactions.is_empty() {
            TransactionLifecycle::RootClosing
        } else {
            TransactionLifecycle::Closing
        };
        // Kept and put back, so a transaction a root-commit listener opens and closes doesn't
        // end the root's phase early. The guard puts it back on a panic too, so a listener
        // that panics leaves the phase as it was.
        let outer = self.transactions.closing.replace(closing);
        // The root's own close, not one a root-commit listener opened: what the transactions
        // removed is final now, so it retires (`defer_retirement`).
        let root = self.transactions.is_empty() && outer.is_none();
        let guard = ClosingPhase { game: self, outer };
        #[cfg(test)]
        guard
            .game
            .trace_storage_attempt(if commit { "commit" } else { "abort" }.into());
        guard.game.close_level(level, commit);
        if root {
            guard.game.world.block_entities.publish_retirement();
        }
    }
    /// What closing `level` does, with the closing phase already set.
    fn close_level(&mut self, mut level: Level, commit: bool) {
        let departed = std::mem::take(&mut level.departed);
        if let Some(scope) = &mut self.block_entity_tick_items {
            scope.close_transaction(commit);
        }
        if let Some(key) = &mut self.block_entity_tick_key {
            key.unbalanced = self.transactions.len() != key.transaction_depth;
        }
        debug_assert_eq!(
            level.dimension,
            Some(self.dim),
            "transaction closed in a different level"
        );
        let lifetimes = std::mem::take(&mut level.lifetimes);
        match (commit, self.transactions.last_mut()) {
            (true, Some(outer)) => {
                outer.actions.merge(level.actions);
                // Slots before whole entities: a slot the outer transaction already kept the
                // whole entity of is older there, but one kept whole only by this child is not,
                // so the child's slot entries go up beside its capture.
                for ((pos, slot), kept) in level.slots {
                    if !outer.block_entities.contains_key(&pos) {
                        outer.slots.entry((pos, slot)).or_insert(kept);
                    }
                }
                for (pos, kept) in level.block_entities {
                    outer.block_entities.entry(pos).or_insert(kept);
                }
                outer
                    .removed_block_entities
                    .extend(level.removed_block_entities);
                for (pos, state) in level.blocks {
                    outer.blocks.entry(pos).or_insert(state);
                }
                for (chunk, randoms) in level.composter_randoms {
                    let kept = outer.composter_randoms.entry(chunk).or_default();
                    for (pos, random) in randoms {
                        kept.entry(pos).or_insert(random);
                    }
                }
                for (pos, biome) in level.biomes {
                    outer.biomes.entry(pos).or_insert(biome);
                }
                for (uuid, kept) in level.players {
                    outer.players.entry(uuid).or_insert(kept);
                }
                for (uuid, kept) in level.player_health {
                    outer.player_health.entry(uuid).or_insert(kept);
                }
                for (id, kept) in level.mob_health {
                    outer.mob_health.entry(id).or_insert(kept);
                }
                for (id, kept) in level.entities {
                    outer.entities.entry(id).or_insert(kept);
                }
                for (id, kept) in level.equipment {
                    outer.equipment.entry(id).or_insert(kept);
                }
                for (id, kept) in level.item_entities {
                    outer.item_entities.entry(id).or_insert(kept);
                }
                for (conn, kept) in level.menus {
                    outer.menus.entry(conn).or_insert(kept);
                }
                for (holder, kept) in level.attachments {
                    outer.attachments.entry(holder).or_insert(kept);
                }
                for (id, kept) in level.travelled {
                    outer.travelled.entry(id).or_insert(kept);
                }
                for (id, slot) in level.in_flight {
                    outer.in_flight.entry(id).or_insert(slot);
                }
                for (id, kept) in level.scratch_kept {
                    outer.scratch_kept.entry(id).or_insert(kept);
                }
                outer.save_gate = outer.save_gate.take().or(level.save_gate);
                for (chunk, mark) in level.chunk_marks {
                    outer.chunk_marks.entry(chunk).or_insert(mark);
                }
                outer.after.extend(level.after);
                // The nearest enclosing transaction of this level takes its entities back; a
                // parent in another level can't, and with none in this level they are told now.
                match (level.dimension)
                    .and_then(|dim| entities::open_in(&mut self.transactions, dim))
                {
                    Some(enclosing) => enclosing.lifetimes.merge(lifetimes),
                    None => self.commit_entity_journal(lifetimes, |_| {}),
                }
            }
            (true, None) => {
                // The commit is in place before the effects run: a join one causes (a placed
                // item's leftover spawning) never announces what the transaction added and removed.
                self.commit_entity_journal(lifetimes, |game| game.root_commit(level));
            }
            (false, _) => {
                // The entities it added go first, so nothing is put back into one.
                let arrivals = self.roll_back_entity_journal(lifetimes);
                let dimension = level.dimension;
                self.roll_back(level);
                if let Some(enclosing) =
                    dimension.and_then(|dim| entities::open_in(&mut self.transactions, dim))
                {
                    enclosing.lifetimes.keep_arrivals(arrivals);
                }
            }
        }
        self.close_departures(departed);
    }
    /// Closes every transaction left open, as aborted. Run before each tick, as a backstop:
    /// `host.transaction` is the only way in, and its drop guard closes each one it opens, so
    /// none is left open by a mod.
    pub(super) fn abort_open_transactions(&mut self) {
        while !self.transactions.is_empty() {
            self.close_storage_transaction(false);
        }
        self.world.block_entities.publish_retirement();
    }

    /// `ComposterWrapper.transactionalRandom`: snapshot only the position's private owner,
    /// including its initial state on a first access that later aborts.
    pub(super) fn take_composter_random(
        &mut self,
        pos: [i32; 3],
    ) -> horizon_core::random::LegacyRandom {
        let epoch = self.native_lifecycle.epoch();
        if self.transactions.composter_epoch != epoch {
            self.clear_composter_randoms();
            self.transactions.composter_epoch = epoch;
        }
        let chunk = (self.dim.key(), (pos[0] >> 4, pos[2] >> 4));
        let key = (pos[0], pos[1], pos[2]);
        let random = self
            .transactions
            .composter_randoms
            .entry(chunk)
            .or_default()
            .remove(&key)
            .unwrap_or_else(|| (*self.world.random).clone());
        if let Some(level) = self.transactions.last_mut() {
            level
                .composter_randoms
                .entry(chunk)
                .or_default()
                .entry(key)
                .or_insert_with(|| random.clone());
        }
        random
    }

    pub(super) fn put_composter_random(
        &mut self,
        pos: [i32; 3],
        random: horizon_core::random::LegacyRandom,
    ) {
        self.transactions
            .composter_randoms
            .entry((self.dim.key(), (pos[0] >> 4, pos[2] >> 4)))
            .or_default()
            .insert((pos[0], pos[1], pos[2]), random);
    }

    /// Retire only an unloading chunk's cached owners and their pending savepoints.
    pub(super) fn forget_composter_chunk(
        &mut self,
        dimension: mod_api::DimensionKey,
        chunk: [i32; 2],
    ) {
        let key = (dimension, (chunk[0], chunk[1]));
        self.transactions.composter_randoms.remove(&key);
        for level in &mut self.transactions.levels {
            level.composter_randoms.remove(&key);
        }
    }

    pub(super) fn clear_composter_randoms(&mut self) {
        self.transactions.composter_epoch = None;
        self.transactions.composter_randoms.clear();
        for level in &mut self.transactions.levels {
            level.composter_randoms.clear();
        }
    }
    /// Keeps the block entity at `pos` as it is, with its attachments and a hopper's
    /// cooldown, if a transaction is open and hasn't yet.
    pub(super) fn journal_block_entity(&mut self, pos: Pos) {
        let Some(level) = self.transactions.last_mut() else {
            return;
        };
        level
            .block_entities
            .entry(pos)
            .or_insert_with(|| Kept::capture(&self.world, &self.hoppers, pos));
    }
    /// Puts back the block entity at `pos` that a transaction removed, as `kept` holds it, with
    /// its generation, where none stands there now.
    fn restore_removed_block_entity(&mut self, pos: Pos, kept: &Kept) {
        let Some(entity) = &kept.entity else {
            return;
        };
        if self.world.block_entities.get(&pos).is_some() {
            return;
        }
        self.world.block_entities.insert(pos, entity.clone());
        if let Some(generation) = kept.generation {
            self.world
                .block_entities
                .restore_generation(pos, generation);
            self.world.block_entities.reattach_retired(pos, generation);
        }
    }
    /// Removes the block entity at `pos`, kept whole by the open transaction, which puts it back
    /// if it aborts: a block write in the transaction that drops it, so later reads in it see
    /// none. Outside a transaction, it just goes.
    pub(super) fn journal_remove_block_entity(&mut self, pos: Pos) {
        self.journal_block_entity(pos);
        if let Some(level) = self.transactions.last_mut() {
            level.removed_block_entities.insert(pos);
        }
        self.world.block_entities.remove(&pos);
    }
    /// How many block entities the open transactions keep whole, and how many single slots:
    /// what a test reads to tell a slot journal from a whole capture without timing it.
    #[cfg(test)]
    pub(in crate::game) fn journal_kept(&self) -> (usize, usize) {
        self.transactions
            .iter()
            .fold((0, 0), |(whole, slots), level| {
                (
                    whole + level.block_entities.len(),
                    slots + level.slots.len(),
                )
            })
    }
    /// Keeps item slot `slot` of the block entity at `pos` as it is, if a transaction is open
    /// and keeps neither it nor the whole entity yet. Only for a write that changes that slot
    /// and nothing else of the entity (no cooldown, cooking or saved data beside it), so the
    /// journal holds one stack however large the entity's saved data is. A slot past the end of
    /// the entity's slot list (a mod's list shorter than its container) is a write that grows
    /// the list: the entity is kept whole instead, length and all.
    pub(super) fn journal_block_entity_slot(&mut self, pos: Pos, slot: usize) {
        let exists = self
            .world
            .block_entities
            .get(&pos)
            .is_some_and(|entity| slot < entity.slots().len());
        if !exists {
            return self.journal_block_entity(pos);
        }
        let Some(level) = self.transactions.last_mut() else {
            return;
        };
        if level.block_entities.contains_key(&pos) {
            return;
        }
        level.slots.entry((pos, slot)).or_insert_with(|| KeptSlot {
            stack: self
                .world
                .block_entities
                .get(&pos)
                .and_then(|entity| entity.slots().get(slot).cloned().flatten()),
            generation: self.world.block_entities.generation(&pos),
        });
    }
    /// Keeps `holder`'s attachment map as it is, if a transaction is open and hasn't yet: a
    /// block entity's with the rest of it, any other's alone.
    pub(super) fn journal_attachments(&mut self, holder: Holder) {
        if let Some(level) = self.transactions.last_mut() {
            level
                .save_gate
                .get_or_insert_with(myriade_engine::join_seq::visibility::PassLease::enter);
            // The chunk that a block entity's or chunk's map marks unsaved (`world_attachments`).
            let chunk = match holder {
                Holder::BlockEntity([x, _, z]) => Some((x >> 4, z >> 4)),
                Holder::Chunk(chunk) => Some((chunk[0], chunk[1])),
                _ => None,
            };
            if let Some(chunk) = chunk {
                let modified = &self.world.modified;
                level
                    .chunk_marks
                    .entry(chunk)
                    .or_insert_with(|| modified.mark(chunk));
            }
        }
        if let Holder::BlockEntity([x, y, z]) = holder {
            return self.journal_block_entity((x, y, z));
        }
        let Some(level) = self.transactions.last() else {
            return;
        };
        if level.attachments.contains_key(&holder) {
            return;
        }
        let entity_bound = if let Holder::Entity(id) = holder {
            std::num::NonZeroU32::new(id).and_then(|id| {
                match self.with_resident_slot(&self.dim.key(), id, |slot| slot.is_bound()) {
                    super::entity_residency::SlotAccess::Done(bound) => Some(bound),
                    _ => None,
                }
            })
        } else {
            None
        };
        let Ok(kept) = self
            .attachment_slot(holder, mod_api::AttachmentAccess::Existing)
            .map(|map| KeptAttachments {
                contents: map.clone(),
                indexed: entity_bound.unwrap_or_else(|| map.has_index_binding()),
            })
        else {
            return;
        };
        if let Some(level) = self.transactions.last_mut() {
            level.attachments.insert(holder, kept);
        }
    }
    /// Keeps the inventory of the player whose state is in `Game` now, `uuid`, as it is, if a
    /// transaction is open and hasn't yet. Called inside `native_as_player`.
    pub(super) fn journal_player(&mut self, uuid: u128) {
        let Some(level) = self.transactions.last_mut() else {
            return;
        };
        level.players.entry(uuid).or_insert_with(|| KeptPlayer {
            ender_items: self.ender_items.clone(),
            player_extra: self.level.player_extra.clone(),
            inventory: self.inventory.clone(),
            armor: self.armor.clone(),
            offhand: self.offhand.clone(),
            equipment: self.equipment.clone(),
            carried: self.carried.clone(),
        });
    }
    /// Called with the target player's body swapped in, before set_health changes it.
    pub(super) fn journal_player_health(&mut self) {
        let uuid = self.native_acting_id();
        let Some(level) = self.transactions.last_mut() else {
            return;
        };
        let Some(player) = self.player.as_ref() else {
            return;
        };
        let std::collections::hash_map::Entry::Vacant(entry) = level.player_health.entry(uuid)
        else {
            return;
        };
        let guest = self.lan_acting.and_then(|conn| {
            let guest = self
                .lan
                .as_ref()?
                .hub
                .guests
                .iter()
                .find(|guest| guest.conn == conn)?;
            Some((conn, guest.death_canceled, guest.death_time))
        });
        entry.insert(KeptPlayerHealth {
            data: player.health.data.clone(),
            events_len: player.events.len(),
            dead: player.health.dead,
            guest,
        });
    }
    pub(super) fn journal_mob_health(&mut self, id: u32) {
        let Some(level) = self.transactions.last_mut() else {
            return;
        };
        if let (Some(health), Some(absorption)) = (self.ecs.health(id), self.ecs.absorption(id)) {
            level.mob_health.entry(id).or_insert(KeptMobHealth {
                health,
                absorption,
                processed: self.died.contains(&id),
            });
        }
    }
    /// Keeps `player`'s menu storage as it is, if a transaction is open and hasn't yet.
    pub(super) fn journal_menus(&mut self, player: mod_api::PlayerRef) {
        let Some(level) = self.transactions.last() else {
            return;
        };
        let Some(super::native_server::PlayerRead::Guest(g)) = self.player_handle_read(player)
        else {
            return;
        };
        let conn = g.conn;
        if level.menus.contains_key(&conn) {
            return;
        }
        let kept = KeptMenus {
            ender_items: g.ender_items.clone(),
            craft_grid: g.craft_grid.clone(),
            native_menu: g.native_menu.clone(),
        };
        if let Some(level) = self.transactions.last_mut() {
            level.menus.insert(conn, kept);
        }
    }
    /// Keeps entity `id`'s inventory as it is, if a transaction is open and hasn't yet.
    pub(super) fn journal_entity(&mut self, id: u32) {
        if self.transactions.is_empty() {
            return;
        }
        let Some(slots) = self.edit_entity_slots(id, |slots, _| slots.to_vec()) else {
            return;
        };
        if let Some(level) = self.transactions.last_mut() {
            level.entities.entry(id).or_insert(slots);
        }
    }
    /// Keeps mob `id`'s equipment as it is, if a transaction is open and hasn't yet (arismoko/horizon#932):
    /// an abort puts every slot back and makes the attribute map over again.
    pub(super) fn journal_equipment(&mut self, id: u32) {
        let stand = self.armor_stand_saved(id);
        let Some(level) = self.transactions.last_mut() else {
            return;
        };
        if level.equipment.contains_key(&id) {
            return;
        }
        let kept = if let Some(slots) = self.ecs.equipment_slots(id) {
            KeptEquipment::Ecs(slots)
        } else if let Some(stand) = stand {
            KeptEquipment::Stand(stand.equipment.iter().cloned().collect())
        } else {
            return;
        };
        level.equipment.insert(id, kept);
    }
    /// `drop_plain` by the player whose state is in `Game` now, `uuid`: at once, or at the root
    /// commit inside a transaction.
    pub(super) fn drop_after_commit(&mut self, uuid: u128, stack: ItemStack) {
        match self.transactions.last_mut() {
            Some(level) => level.after.push(After::PlayerDrop(uuid, stack)),
            None => {
                self.drop_plain(stack);
            }
        }
    }
    /// Sets the block at `pos` to `state`. In a transaction, stored only, keeping the old
    /// state, and set with its updates at the root commit; otherwise at once
    /// (`Level.setBlock`).
    pub(super) fn journal_set_block(&mut self, pos: [i32; 3], state: BlockStateId) -> bool {
        let Some(level) = self.transactions.last_mut() else {
            return self.level_set(pos, state).is_ok();
        };
        let key = (pos[0], pos[1], pos[2]);
        let old = self.world.block(pos[0], pos[1], pos[2]);
        level.blocks.entry(key).or_insert(old);
        level.after.push(After::Block(key));
        self.world.set_stored(pos[0], pos[1], pos[2], state)
    }
    /// Keep the original cell without publishing the tentative write.
    pub(in crate::game) fn journal_biome(&mut self, pos: [i32; 3], old: u16) {
        if let Some(level) = self.transactions.last_mut() {
            level
                .biomes
                .entry((pos[0] >> 2, pos[1] >> 2, pos[2] >> 2))
                .or_insert(old);
        }
    }
    /// Queue a reusable interaction effect for the outermost successful commit.
    pub(super) fn defer_effect(&mut self, effect: DeferredEffect) {
        self.after_commit(After::Effect(effect));
    }
    /// Runs `after` at once, or at the root commit inside a transaction.
    pub(super) fn after_commit(&mut self, after: After) {
        match self.transactions.last_mut() {
            Some(level) => level.after.push(after),
            None => self.run_after(after),
        }
    }
    fn run_after(&mut self, after: After) {
        match after {
            After::Payload(payload) => payload.publish(),
            After::Cauldron(effects) => self.commit_cauldron_effects(effects),
            After::Composter(effects) => self.commit_composter_effects(effects),
            After::FluidWrite(written) => self.commit_fluid_write(*written),
            After::FluidEffects(effects) => self.commit_fluid_effects(*effects),
            After::Changed(pos) => self.container_changed(pos),
            After::Capabilities(pos) => self.world.capability_generations.invalidate(pos),

            After::BreakProgress(breaker, pos, progress) => {
                self.server_destroy_block_progress(breaker, pos, progress);
            }
            After::Biome(key) => {
                self.commit_biome_columns(&std::collections::HashSet::from([key]), &[key]);
            }
            After::Effect(effect) => match effect {
                DeferredEffect::CustomStat {
                    player,
                    key,
                    amount,
                } => {
                    self.native_as_player(player, |g| g.award_custom_stat(key, amount));
                }
                DeferredEffect::ItemStat {
                    player,
                    stat,
                    item,
                    amount,
                } => {
                    self.native_as_player(player, |g| g.award_stat(stat, &item, amount));
                }
                DeferredEffect::Sound {
                    sound,
                    at,
                    volume,
                    pitch,
                } => self.audio.play(sound, Some(at), volume, pitch),
            },
            // Only reached through a root commit, which sets the block itself.
            After::Block(_) => {}
            After::Drop(pos, stack) => {
                self.spawn_item(stack, pos.map(|p| f64::from(p) + 0.5), [0.0; 3], 10);
            }
            After::GivePickup(uuid) => {
                self.native_as_player(uuid, |g| g.give_pickup());
            }
            After::InventoryTrigger(uuid) => {
                self.native_as_player(uuid, |g| {
                    g.check_inventory_trigger();
                });
            }
            After::GiveRest(uuid, stack, owner) => {
                self.native_as_player(uuid, |g| g.give_remainder(stack, owner));
            }
            After::PlayerEquip(uuid, slot, stack) => {
                self.native_as_player(uuid, |g| {
                    g.player_equipped(&slot, stack.as_ref());
                    g.apply_equipment();
                });
            }
            After::PlayerDrop(uuid, stack) => {
                // A player who left meanwhile drops nothing; their inventory had changed for good.
                self.native_as_player(uuid, |g| {
                    g.drop_plain(stack);
                });
            }
            After::Spawn(at, stack) => {
                self.spawn_item(stack, at, [0.0; 3], 10);
            }
            After::Emptied(id) => {
                if self.ecs.item(id).is_some_and(|it| it.stack.count == 0) {
                    self.ecs_remove_item(id);
                }
            }
            After::Equip {
                at,
                sound,
                equips,
                who,
            } => {
                // `equip_item_effects` rebuilt from the entity it names: the
                // sculk context reads the mob as it is at the root commit.
                let ctx = self.who_context(Some(myriade_engine::mobs::Who::Mob(who)), None);
                self.equip_item_effects(at, sound, equips, ctx);
            }
        }
    }
    fn root_commit(&mut self, level: Level) {
        let mut told = std::collections::HashSet::new();
        let mut biome_columns = std::collections::HashSet::new();
        for after in level.after {
            match after {
                // Each storage tells the world once, after every write.
                After::Changed(pos) if !told.insert(Ok(pos)) => {}
                After::Block(pos) => {
                    if !told.insert(Err(pos)) {
                        continue;
                    }
                    let now = self.world.block(pos.0, pos.1, pos.2);
                    let was = level.blocks[&pos];
                    // A block entity the transaction removed comes back where a later write
                    // there put back a block of the kind that had it.
                    let b = self.res.behaviours();
                    if level.removed_block_entities.contains(&pos)
                        && b.block(now) == b.block(was)
                        && let Some(kept) = level.block_entities.get(&pos)
                    {
                        self.restore_removed_block_entity(pos, kept);
                    }
                    if now != was {
                        // The stored state back, so the one real write sees the change.
                        self.world.set_stored(pos.0, pos.1, pos.2, was);
                        let _ = self.level_set([pos.0, pos.1, pos.2], now);
                    }
                }
                After::Biome(key) if !biome_columns.insert(key) => {}
                after => self.run_after(after),
            }
        }
        // A crafting grid the transaction changed has its result made again, once for each
        // player however many writes it took (`InventoryMenu.slotsChanged`).
        for (conn, kept) in &level.menus {
            let mut guests = self.lan.iter().flat_map(|host| host.hub.guests.iter());
            let changed =
                guests.any(|g| g.placed && g.conn == *conn && g.craft_grid != kept.craft_grid);
            if changed {
                self.guest_craft_result(*conn);
            }
        }
        for (pos, kept) in level.block_entities {
            let record = |entity: Option<&BlockEntity>| match entity {
                Some(BlockEntity::Jukebox(j)) => j.record().cloned(),
                _ => None,
            };
            let was = record(kept.entity.as_ref());
            let now = record(self.world.block_entities.get(&pos));
            if was == now {
                continue;
            }
            // The song of the record it had stops; the one it has now starts.
            let pos = [pos.0, pos.1, pos.2];
            if was.is_some() {
                self.jukebox_stopped(pos);
            }
            if now.is_some() {
                self.jukebox_inserted(pos);
            }
        }
    }
    fn roll_back(&mut self, mut level: Level) {
        self.roll_back_in_flight(&mut level);
        self.scratch_roll_back(std::mem::take(&mut level.scratch_kept));
        // Discard deferred effects before undoing tentative action storage.
        level.after.clear();
        // Each mob's equipment goes back as it was, and the attribute map is
        // made over it again (arismoko/horizon#932).
        let equipment = std::mem::take(&mut level.equipment);
        for (id, kept) in equipment {
            match kept {
                KeptEquipment::Ecs(stacks) => {
                    for (slot, stack) in stacks.into_iter().enumerate() {
                        self.ecs.put_equipment_slot(id, slot, stack);
                    }
                }
                // An armor stand keeps no live attribute map of its gear; the
                // plain write puts its slots back as they were.
                KeptEquipment::Stand(stacks) => {
                    self.armor_stand_edit(id, |stand| {
                        for (slot, stack) in stacks.into_iter().enumerate() {
                            stand.equipment.set(slot, stack);
                        }
                    });
                }
            }
        }

        for (pos, kept) in level.block_entities {
            match (&kept.entity, self.world.block_entities.get_mut(&pos)) {
                (Some(entity), Some(now)) => {
                    *now = entity.clone();
                    self.world
                        .block_entities
                        .restore_generation(pos, kept.generation.unwrap());
                }
                // One this transaction removed comes back.
                (Some(_), None) if level.removed_block_entities.contains(&pos) => {
                    self.restore_removed_block_entity(pos, &kept);
                }
                _ => {}
            }
            self.world
                .block_entities
                .put_back_attachments(pos, kept.generation, kept.attachments);
            match kept.name {
                Some(name) => self.world.block_names.insert(pos, name),
                None => self.world.block_names.remove(&pos),
            };
            self.world.set_block_entity_extras(pos, kept.extras);
            match kept.hopper {
                Some(hopper) => self.hoppers.insert(pos, hopper),
                None => self.hoppers.remove(&pos),
            };
        }
        // After the whole entities: a slot kept here was kept before any whole capture of its
        // entity at this depth, so its stack is the older one. Only into the block entity it
        // was taken from: one a write replaced (a `set_block` in the transaction) is left as is.
        for ((pos, slot), kept) in level.slots {
            if self.world.block_entities.generation(&pos) != kept.generation {
                continue;
            }
            if let Some(target) = self
                .world
                .block_entities
                .get_mut(&pos)
                .and_then(|entity| entity.slots_mut().get_mut(slot))
            {
                *target = kept.stack;
                #[cfg(test)]
                if let Some(trace) = &self.transactions.trace {
                    trace
                        .lock()
                        .unwrap()
                        .push(format!("restore:{},{},{}:{slot}", pos.0, pos.1, pos.2));
                }
            }
        }
        let bottom_quart = self.world.bottom_y() >> 2;
        for ((qx, qy, qz), biome) in level.biomes {
            if let Some(chunk) = self.world.chunks.get_mut(&(qx >> 2, qz >> 2)) {
                std::sync::Arc::make_mut(chunk).set_biome_cell(
                    qx & 3,
                    qy - bottom_quart,
                    qz & 3,
                    biome,
                );
            }
        }
        for (pos, state) in level.blocks {
            self.world.set_stored(pos.0, pos.1, pos.2, state);
        }
        for (chunk, randoms) in level.composter_randoms {
            self.transactions
                .composter_randoms
                .entry(chunk)
                .or_default()
                .extend(randoms);
        }
        for (id, kept) in level.entities {
            self.edit_entity_slots(id, |slots, _| {
                for (slot, kept) in slots.iter_mut().zip(kept) {
                    *slot = kept;
                }
            });
        }
        for (id, kept) in level.item_entities {
            self.ecs.set_item_stack(id, kept);
        }
        if let Some(host) = self.lan.as_mut() {
            host.hub.guests.update_all(|g| {
                if let Some(kept) = level.menus.get(&g.conn).filter(|_| g.placed) {
                    g.ender_items.clone_from(&kept.ender_items);
                    g.craft_grid.clone_from(&kept.craft_grid);
                    g.native_menu.clone_from(&kept.native_menu);
                }
            });
        }
        for (holder, kept) in level.attachments {
            if let Ok(map) = self.attachment_slot(holder, mod_api::AttachmentAccess::Existing) {
                map.replace_contents(kept.contents);
                if !kept.indexed {
                    map.unbind_index();
                }
            }
            if !kept.indexed
                && let Holder::Entity(id) = holder
                && let Some(id) = std::num::NonZeroU32::new(id)
            {
                self.with_resident_slot(&self.dim.key(), id, |slot| {
                    *slot = Default::default();
                });
            }
        }
        self.restore_travelled(std::mem::take(&mut level.travelled));
        // After the maps: putting a chunk's map back marks it again.
        for (chunk, mark) in level.chunk_marks {
            self.world.modified.restore(chunk, mark);
        }
        for (id, kept) in level.mob_health {
            self.ecs.set_health(id, kept.health);
            self.ecs.set_absorption(id, kept.absorption);
            if kept.processed {
                self.died.insert(id);
            } else {
                self.died.remove(&id);
            }
        }
        for (uuid, kept) in level.player_health {
            // Restoration must also reach a body tentatively set to zero health:
            // native_as_player deliberately refuses dead bodies for public writes.
            let target = if uuid == self.native_acting_id() {
                Some(super::commands::Target::Player)
            } else {
                self.lan
                    .as_ref()
                    .and_then(|host| host.hub.holder_guest(uuid, self.dim.id()))
                    .map(|guest| super::commands::Target::Guest(guest.conn))
            };
            if let Some(target) = target {
                self.for_player(target, |g| {
                    if let Some(player) = g.player.as_mut() {
                        player.health.data = kept.data;
                        player.events.truncate(kept.events_len);
                        player.health.dead = kept.dead;
                    }
                    if let Some((conn, canceled, time)) = kept.guest
                        && let Some(host) = g.lan.as_mut()
                        && let Some(gi) =
                            host.hub.guests.iter().position(|guest| guest.conn == conn)
                    {
                        host.hub.guests.update(gi, |guest| {
                            guest.death_canceled = canceled;
                            guest.death_time = time;
                        });
                    }
                });
            }
        }
        level.actions.rollback_in(self);
        for (uuid, kept) in level.players {
            self.native_as_player(uuid, |g| {
                g.ender_items = kept.ender_items;
                g.level.player_extra = kept.player_extra;
                g.inventory = kept.inventory;
                g.armor = kept.armor;
                g.offhand = kept.offhand;
                g.equipment = kept.equipment;
                g.carried = kept.carried;
            });
        }
    }
}

include!("transactions/item_entity.rs");


#[cfg(test)]
mod tests {
    use super::super::api_values::TryIntoApi;
    use super::super::{BlockEntity, Game, Options, Progress, Settings, load_resources};
    use myriade_engine::{chunk::Chunk, items::ItemStack};
    use mod_api::HostTransaction;
    use mod_api::ServerHost;
    use mod_api::{
        BlockSide, BlockUpdateFlags, ITEMS, ItemResource, Transaction,
    };
    use std::{sync::Arc, time::Instant};

    #[test]
    #[ignore = "daily"]
    fn cauldron_stat_listener_observes_committed_state_and_can_cancel() {
        use super::super::native_entity_refs::tests::run_fresh_cases;
        use crate::game::api_values::TryIntoApi;
        use mod_api::{
            Flow, Hooks, ItemSpawnEvent, ItemTossEvent, ModEntry, Mods, PlayerStatAwardEvent,
        };
        use std::sync::Mutex;
        if run_fresh_cases(
            "game::transactions::tests::cauldron_stat_listener_observes_committed_state_and_can_cancel",
            "HORIZON_CAULDRON_STAT_CHILD",
            &["run"],
        ) {
            return;
        }
        struct Listener {
            seen: Arc<Mutex<Vec<(Option<mod_api::BlockState>, Option<String>)>>>,
            drops: Arc<Mutex<Vec<(&'static str, Option<mod_api::BlockState>)>>>,
        }
        #[mod_api::__subscriptions]
        impl Hooks for Listener {
            fn player_stat_award(
                &self,
                host: &mut dyn ServerHost,
                event: &mut PlayerStatAwardEvent<'_>,
            ) -> Flow {
                if event.stat.as_str() != "minecraft:fill_cauldron" {
                    return Flow::Continue;
                }
                let level = host.level("minecraft:overworld").unwrap();
                let hand = mod_api::ItemPlace::PlayerHand {
                    player: event.player,
                    hand: mod_api::client::first_person::Hand::Main,
                };
                self.seen.lock().unwrap().push((
                    level.block_state(host, [2, 64, 2]),
                    hand.stack(host).map(|s| {
                        mod_api::__private::item_key_id(s.item())
                            .unwrap()
                            .to_string()
                    }),
                ));
                Flow::Cancel
            }
            fn item_toss(&self, host: &mut dyn ServerHost, event: &mut ItemTossEvent) -> Flow {
                let potion = event.item.stack(host).is_some_and(|stack| {
                    mod_api::__private::item_key_id(stack.item())
                        .is_some_and(|id| id == "minecraft:potion")
                });
                if potion {
                    let level = host.level("minecraft:overworld").unwrap();
                    self.drops
                        .lock()
                        .unwrap()
                        .push(("toss", level.block_state(host, [2, 64, 2])));
                }
                Flow::Continue
            }
            fn item_spawn(&self, host: &mut dyn ServerHost, event: &mut ItemSpawnEvent<'_>) {
                if event.item == "minecraft:potion" {
                    let level = host.level("minecraft:overworld").unwrap();
                    self.drops
                        .lock()
                        .unwrap()
                        .push(("spawn", level.block_state(host, [2, 64, 2])));
                }
            }
        }
        let seen = Arc::new(Mutex::new(Vec::new()));
        let drops = Arc::new(Mutex::new(Vec::new()));
        let mods: Mods = vec![ModEntry::new(
            "observer",
            Box::new(Listener {
                seen: seen.clone(),
                drops: drops.clone(),
            }),
        )]
        .into();
        mod_platform::__private::install(mods);
        super::super::native_guest_turn_tests::with_guests(|game| {
            let pos = [2, 64, 2];
            game.world
                .chunks
                .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
            let empty = game
                .pack
                .assets
                .registry
                .default_state("minecraft:cauldron")
                .unwrap();
            let full = game
                .pack
                .assets
                .registry
                .resolve(&horizon_world::chunk::BlockState {
                    name: "minecraft:water_cauldron".into(),
                    properties: [("level".into(), "3".into())].into_iter().collect(),
                })
                .unwrap();
            game.world.set(pos[0], pos[1], pos[2], empty).unwrap();
            game.as_guest(12, |game| {
                game.selected = 0;
                game.creative = false;
                game.inventory[0] = Some(ItemStack::new("minecraft:water_bucket", 1));
                game.open_storage_transaction();
                assert!(game.use_on_cauldron(pos).unwrap());
                assert!(seen.lock().unwrap().is_empty());
                game.close_storage_transaction(false);
                assert!(seen.lock().unwrap().is_empty());
                assert!(game.use_on_cauldron(pos).unwrap());
                assert_eq!(
                    seen.lock().unwrap().as_slice(),
                    &[(
                        Some(empty.try_into_api().unwrap()),
                        Some("minecraft:bucket".into())
                    )]
                );
                game.world.set(pos[0], pos[1], pos[2], empty).unwrap();
                game.inventory[0] = Some(ItemStack::new("minecraft:water_bucket", 1));
                game.open_storage_transaction();
                assert!(game.use_on_cauldron(pos).unwrap());
                assert_eq!(seen.lock().unwrap().len(), 1);
                game.close_storage_transaction(true);
            })
            .unwrap();
            assert_eq!(
                *seen.lock().unwrap(),
                vec![
                    (
                        Some(empty.try_into_api().unwrap()),
                        Some("minecraft:bucket".into())
                    ),
                    (
                        Some(full.try_into_api().unwrap()),
                        Some("minecraft:bucket".into())
                    )
                ]
            );
            let level2 = game
                .pack
                .assets
                .registry
                .resolve(&horizon_world::chunk::BlockState {
                    name: "minecraft:water_cauldron".into(),
                    properties: [("level".into(), "2".into())].into_iter().collect(),
                })
                .unwrap();
            game.as_guest(12, |game| {
                let setup = |game: &mut Game<'_>| {
                    game.world.set(pos[0], pos[1], pos[2], full).unwrap();
                    game.inventory
                        .iter_mut()
                        .for_each(|s| *s = Some(ItemStack::new("minecraft:stone", 64)));
                    game.offhand = Some(ItemStack::new("minecraft:stone", 64));
                    game.inventory[0] = Some(ItemStack::new("minecraft:glass_bottle", 2));
                };
                setup(game);
                assert!(game.use_on_cauldron(pos).unwrap());
                assert_eq!(
                    drops.lock().unwrap().as_slice(),
                    &[
                        ("toss", Some(full.try_into_api().unwrap())),
                        ("spawn", Some(full.try_into_api().unwrap()))
                    ]
                );
                drops.lock().unwrap().clear();
                setup(game);
                game.open_storage_transaction();
                assert!(game.use_on_cauldron(pos).unwrap());
                assert!(drops.lock().unwrap().is_empty());
                game.close_storage_transaction(false);
                assert!(drops.lock().unwrap().is_empty());
                game.open_storage_transaction();
                assert!(game.use_on_cauldron(pos).unwrap());
                game.close_storage_transaction(true);
                assert_eq!(
                    drops.lock().unwrap().as_slice(),
                    &[
                        ("toss", Some(level2.try_into_api().unwrap())),
                        ("spawn", Some(level2.try_into_api().unwrap()))
                    ]
                );
            })
            .unwrap();
            assert_eq!(
                game.lan.as_ref().unwrap().play[&12]
                    .stats
                    .get("custom", "minecraft:fill_cauldron"),
                0
            );
        });
    }

    #[test]
    #[ignore = "daily"]
    fn cauldron_effects_wait_for_root_and_abort_with_inventory() {
        crate::game::native_guest_turn_tests::with_acting_guest(horizon_core::geometry::Vec3::new(0.5, 65.0, 3.5), |game| {
            let pos = [2, 64, 2];
            game.world
                .chunks
                .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
            let empty = game
                .pack
                .assets
                .registry
                .default_state("minecraft:cauldron")
                .unwrap();
            game.world.set(pos[0], pos[1], pos[2], empty).unwrap();
            game.selected = 0;
            game.creative = false;
            game.inventory[0] = Some(ItemStack::new("minecraft:water_bucket", 1));
            let before = game.stats.get("custom", "minecraft:fill_cauldron");
            game.audio.capture = Some(Vec::new());
            game.open_storage_transaction();
            assert!(game.use_on_cauldron(pos).unwrap());
            assert_eq!(game.stats.get("custom", "minecraft:fill_cauldron"), before);
            assert!(game.audio.capture.as_ref().unwrap().is_empty());
            game.close_storage_transaction(false);
            assert_eq!(game.world.block(pos[0], pos[1], pos[2]), empty);
            assert_eq!(
                &*game.inventory[0].as_ref().unwrap().item,
                "minecraft:water_bucket"
            );
            assert_eq!(game.stats.get("custom", "minecraft:fill_cauldron"), before);
            assert!(game.audio.capture.as_ref().unwrap().is_empty());
            game.open_storage_transaction();
            game.open_storage_transaction();
            assert!(game.use_on_cauldron(pos).unwrap());
            game.close_storage_transaction(false);
            assert_eq!(game.world.block(pos[0], pos[1], pos[2]), empty);
            assert_eq!(
                &*game.inventory[0].as_ref().unwrap().item,
                "minecraft:water_bucket"
            );
            game.close_storage_transaction(true);
            assert_eq!(game.stats.get("custom", "minecraft:fill_cauldron"), before);
            assert!(game.audio.capture.as_ref().unwrap().is_empty());
            assert!(game.use_on_cauldron(pos).unwrap());
            assert_ne!(game.world.block(pos[0], pos[1], pos[2]), empty);
            assert_eq!(
                &*game.inventory[0].as_ref().unwrap().item,
                "minecraft:bucket"
            );
            assert_eq!(
                game.stats.get("custom", "minecraft:fill_cauldron"),
                before + 1
            );
            assert_eq!(
                game.audio
                    .capture
                    .as_ref()
                    .unwrap()
                    .iter()
                    .map(|s| s.event.as_str())
                    .collect::<Vec<_>>(),
                ["minecraft:item.bucket.empty"]
            );
        });
    }

    #[test]
    #[ignore = "daily"]
    fn a_menu_write_outside_a_transaction_keeps_no_journal() {
        super::super::native_guest_turn_tests::with_guests(|game| {
            let player = game.player(12).unwrap();
            let menu = game.player_container_menu(player).unwrap();
            let stone = ItemStack::new("minecraft:stone", 4);
            game.journal_menus(player);
            *game.native_menu_storage(menu, 1).unwrap() = Some(stone.clone());
            assert!(game.transactions.is_empty());
            assert_eq!(
                menu.slot(game, 1),
                Some(Some(stone.try_into_api().unwrap()))
            );
        });
    }

    #[test]
    #[ignore = "daily"]
    fn menu_journals_keep_only_the_first_write_inside_a_transaction() {
        super::super::native_guest_turn_tests::with_guests(|game| {
            let player = game.player(12).unwrap();
            let menu = game.player_container_menu(player).unwrap();
            let stone = ItemStack::new("minecraft:stone", 4);
            *game.native_menu_storage(menu, 1).unwrap() = Some(stone.clone());
            let place = mod_api::ItemPlace::MenuSlot { menu, slot: 1 };
            let resource = ItemResource::of(&stone.try_into_api().unwrap());
            let aborted: Result<(), ()> = game.transaction(|game, tx| {
                assert_eq!(place.insert(game, tx, &resource, 2), 2);
                assert_eq!(place.insert(game, tx, &resource, 3), 3);
                let journal = &game.transactions.last().unwrap().menus;
                assert_eq!(journal.len(), 1);
                assert_eq!(journal[&12].craft_grid[0], Some(stone.clone()));
                assert_eq!(menu.slot(game, 1).unwrap().unwrap().count(), 9);
                Err(())
            });
            assert_eq!(aborted, Err(()));
            assert!(game.transactions.is_empty());
            assert_eq!(
                menu.slot(game, 1),
                Some(Some(stone.try_into_api().unwrap()))
            );
        });
    }

    fn insert(
        host: &mut dyn ServerHost,
        at: BlockSide,
        slot: usize,
        stack: ItemStack,
        tx: &Transaction,
    ) -> Option<ItemStack> {
        let handler = at
            .level
            .capability(host, &ITEMS, at.pos, at.side)
            .expect("item capability");
        let inserted = handler.insert(
            host,
            tx,
            slot,
            &ItemResource::of(&stack.try_into_api().unwrap()),
            stack.count,
        );
        (inserted < stack.count).then(|| ItemStack {
            count: stack.count - inserted,
            ..stack
        })
    }

}

#[cfg(test)]
mod mod_capability_tests {
    use super::super::native_entity_refs::tests::with_game;
    use myriade_engine::chunk::Chunk;
    use myriade_engine::containers::{BlockEntity, ModBlockEntity, ModBlockEntityFields};
    use horizon_core::nbt::{Compound, Tag};
    use mod_api::ServerHost;
    use mod_api::{BlockSide, HostTransaction, ResourceHandler, move_resources};
    use mod_platform::__private::part_tests::{CHEMICALS, ChemTank, Chemical};
    use std::sync::Arc;

    /// Two mods sharing a capability one of them defines, as Mekanism's chemicals are shared
    /// through its API: the first's tank and the second's vat each answer `CHEMICALS`, and a
    /// move between them in a committed transaction moves the chemical, kept in their saved
    /// data; in an aborted one, both are put back.
    #[test]
    #[ignore = "daily"]
    fn a_chemical_moves_between_two_mods_parts_and_an_abort_puts_it_back() {
        use super::super::native_entity_refs::tests::{declared_part_root, with_game_at};
        let Some(root) = declared_part_root(
            "game::transactions::mod_capability_tests::a_chemical_moves_between_two_mods_parts_and_an_abort_puts_it_back",
        ) else {
            return;
        };
        with_game_at(root, |game| {
            game.world
                .chunks
                .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
            let level = game.level("minecraft:overworld").unwrap();
            let hydrogen = Chemical("test:hydrogen".into());
            let mut nbt = Compound::new();
            nbt.insert("chemical".into(), Tag::String("test:hydrogen".into()));
            nbt.insert("amount".into(), Tag::Int(600));
            for (x, id, nbt) in [
                (0, "test:chem_tank", nbt),
                (1, "test:chem_vat", Compound::new()),
            ] {
                let state = game.pack.assets.registry.default_state(id).unwrap();
                game.world.set(x, 64, 0, state).unwrap();
                let fields = ModBlockEntityFields {
                    nbt,
                    ..Default::default()
                };
                let entity = BlockEntity::Mod(Box::new(ModBlockEntity::new(id.into(), fields)));
                game.world.block_entities.insert((x, 64, 0), entity);
            }
            let host: &mut dyn ServerHost = game;
            let tank = level
                .capability(host, &CHEMICALS, [0, 64, 0], None)
                .unwrap();
            let vat = level
                .capability(host, &CHEMICALS, [1, 64, 0], None)
                .unwrap();
            // Each answered by its own mod.
            assert_ne!(tank.owner(), vat.owner());
            assert!(![tank.owner(), vat.owner()].contains(&"the game".to_owned()));
            let amounts = |host: &mut dyn ServerHost| {
                let side = |x| BlockSide {
                    level,
                    pos: [x, 64, 0],
                    side: None,
                };
                (
                    ChemTank(side(0)).amount(host, 0),
                    ChemTank(side(1)).amount(host, 0),
                )
            };

            let moved = host.transaction(|host, tx| {
                move_resources(host, &tank, &vat, |c| *c == hydrogen, 250, tx).map_err(drop)
            });
            assert_eq!(moved, Ok(250));
            assert_eq!(amounts(host), (350, 250));
            assert_eq!(vat.resource(host, 0), Some(hydrogen.clone()));

            let aborted = host.transaction(|host, tx| {
                let moved = move_resources(host, &tank, &vat, |_| true, 100, tx);
                assert_eq!(moved.ok(), Some(100));
                assert_eq!(amounts(host), (250, 350));
                Err::<(), ()>(())
            });
            assert!(aborted.is_err());
            assert_eq!(amounts(host), (350, 250));
            assert!(game.transactions.is_empty());
        });
    }

    /// A mod's attachments on an entity and on a player are kept by a transaction as a block
    /// entity's are: a commit keeps what it wrote, an abort puts each map back as it was before
    /// the transaction first reached it (none for an entity that had none), and a guest's is
    /// put back wherever their state lives at the abort, swapped in for their turn or not.
    #[test]
    #[ignore = "daily"]
    fn entity_and_player_attachments_stay_on_commit_and_go_back_on_abort() {
        use myriade_engine::entities::ArmorStand;
        use horizon_core::geometry::Vec3;
        use mod_api::{AttachmentType, Holder};
        const ENERGY: AttachmentType<i32> = AttachmentType {
            namespace: "robot",
            name: "energy",
            default: || 0,
            codec: None,
            copy_on_death: false,
            sync: false,
        };
        with_game(|game| {
            game.world
                .chunks
                .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
            crate::game::ecs::test_support::place_test_stand(game, 13, &ArmorStand::new(13, Vec3::ZERO, 0.0));
            crate::game::ecs::test_support::place_test_stand(game, 14, &ArmorStand::new(14, Vec3::ZERO, 0.0));
            let guest = 0x5eed_u128;
            let config = myriade_engine::lan::hub::HubConfig {
                default_game_mode: 0,
                guest_commands: false,
                force_game_mode: false,
                hardcore: false,
                view_distance: 10,
                simulation_distance: 10,
                dimension: "minecraft:overworld".into(),
                allow_flight: false,
                seed: 0,
                max_players: 20,
                levels: Default::default(),
                level_seeds: Default::default(),
            };
            game.lan_host(
                super::super::lan::LanSettings {
                    port: 0,
                    guest_commands: false,
                    force_game_mode: false,
                },
                myriade_engine::lan::NetConfig {
                    announce: false,
                    ..myriade_engine::lan::NetConfig::lan(0, "test".into(), 2)
                },
                config,
            )
            .unwrap();
            let level = game.level(game.dim.id()).unwrap();
            let player_ref = game.fake_player(level, guest, "test").unwrap();
            let conn = game
                .lan
                .as_ref()
                .unwrap()
                .hub
                .guests
                .iter()
                .find(|g| g.entity_id as u32 == player_ref.entity().id())
                .unwrap()
                .conn;
            let (stand, fresh, player) = (
                Holder::Entity(13),
                Holder::Entity(14),
                Holder::Player(guest),
            );
            let energy =
                |host: &mut dyn ServerHost, holder| ENERGY.get_existing(host, holder).unwrap();

            let host: &mut dyn ServerHost = game;
            let done = host.transaction(|host, _| {
                ENERGY.set(host, stand, 100)?;
                ENERGY.set(host, player, 40)
            });
            assert_eq!(done, Ok(()));
            assert_eq!(
                (energy(host, stand), energy(host, player)),
                (Some(100), Some(40))
            );

            // Aborted, with a committed child inside: both maps as they were, and the entity
            // that had no value has none.
            let aborted: Result<(), _> = host.transaction(|host, _| {
                ENERGY.set(host, stand, 250)?;
                ENERGY.set(host, player, 90)?;
                ENERGY.set(host, fresh, 7)?;
                host.transaction(|host, _| ENERGY.set(host, stand, 300))?;
                assert_eq!(energy(host, stand), Some(300));
                Err(mod_api::AttachmentError::MissingHolder)
            });
            assert!(aborted.is_err());
            assert_eq!(
                (energy(host, stand), energy(host, player)),
                (Some(100), Some(40))
            );
            assert_eq!(energy(host, fresh), None);

            // Written while the guest's state is swapped in for their turn, aborted after it
            // is swapped back out: the guest's own map is put back.
            game.open_storage_transaction();
            game.lan_acting_uuid = Some(guest);
            std::mem::swap(
                &mut game.player_state.attachments,
                game.lan
                    .as_mut()
                    .unwrap()
                    .hub
                    .guests
                    .attachments_mut(conn)
                    .unwrap(),
            );
            let host: &mut dyn ServerHost = game;
            ENERGY.set(host, player, 70).unwrap();
            std::mem::swap(
                &mut game.player_state.attachments,
                game.lan
                    .as_mut()
                    .unwrap()
                    .hub
                    .guests
                    .attachments_mut(conn)
                    .unwrap(),
            );
            game.lan_acting_uuid = None;
            game.close_storage_transaction(false);
            let host: &mut dyn ServerHost = game;
            assert_eq!(energy(host, player), Some(40));
            assert!(game.transactions.is_empty());
        });
    }
}
