//! The world's block entities and the order they tick in: vanilla's
//! `Level.blockEntityTickers`, kept beside the block entity map so a tick
//! visits only the block entities that tick, in vanilla's order, instead of
//! scanning the whole map once per kind.
//!
//! The list follows `LevelChunk.updateBlockEntityTicker` and
//! `Level.tickBlockEntities`:
//! - a block entity whose block state has a ticker gets a slot at the end of
//!   the list (through `pendingBlockEntityTickers`, so a slot made during a
//!   tick first ticks on the next one);
//! - replacing the block entity at a position (`setBlockEntity`), or changing
//!   its block state to another state with a ticker, keeps its slot
//!   (`RebindableTickingBlockEntityWrapper.rebind`);
//! - removing it (`removeBlockEntity`), unloading its chunk
//!   (`clearAllBlockEntities`) or changing its state to one without a ticker
//!   (a jukebox losing its record) frees the slot; a later ticker there is
//!   appended again;
//! - a loaded chunk's block entities are added in the iteration order of its
//!   `Object2ObjectOpenHashMap<BlockPos, BlockEntity>`
//!   (`registerAllBlockEntitiesAfterLevelLoad`), filled in the order the chunk
//!   listed them.
//!
//! Which states tick is a table by state id, built by the game from the block
//! registry; whether a slot is made is decided when the slot is promoted at
//! the start of the next tick, from the block state then.
use crate::chunk::block_entity::{BlockEntity};
use crate::fast_hash::{FxHashMap, FxHashSet};
use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

mod carry;
pub use carry::Carried;

static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);

fn next_token() -> u64 {
    let mut token = NEXT_TOKEN.load(Ordering::Relaxed);
    loop {
        let next = token
            .checked_add(1)
            .expect("block entity token space exhausted");
        match NEXT_TOKEN.compare_exchange_weak(token, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return token,
            Err(current) => token = current,
        }
    }
}

/// Exclusive authority to return one instance taken from a block entity store.
/// Invalidated by removal, replacement, unloading, or the next ticker sweep.
/// Dropping it does not restore the instance; it cannot be cloned or forged.
pub struct BlockEntityLease {
    pos: Pos,
    token: u64,
    generation: u64,
}

pub type Pos = (i32, i32, i32);

/// What a block state's block entity does each tick (`BlockState.getTicker`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Ticking {
    /// No ticker, and the block has no block entity that could have one.
    #[default]
    No,
    /// A ticker.
    Yes,
    /// The block's block entity, but this state has no ticker (a jukebox
    /// without a record): a state change to it frees the slot.
    Idle,
}

#[derive(Default)]
pub struct BlockEntities {
    map: HashMap<Pos, BlockEntity>,
    generations: FxHashMap<Pos, u64>,
    on_load: std::collections::VecDeque<(Pos, u64)>,
    on_load_enabled: bool,/// `Level.blockEntityTickers`: positions and the slot each was given.
    tickers: Arc<Vec<(Pos, u32)>>,
    /// `LevelChunk.tickersInLevel`: the live slot of each ticking position.
    live: FxHashMap<Pos, u32>,
    /// Positions that may need a slot, in the order vanilla would have made
    /// it (`pendingBlockEntityTickers`), each with a token; decided at the
    /// next promotion from the block state then.
    added: Vec<(Pos, u32)>,
    /// The token of each position's pending entry: an entry whose token is
    /// not here any more was cancelled.
    pending: FxHashMap<Pos, u32>,
    next_slot: u32,
    /// Whether a slot was freed since the list was last swept: otherwise
    /// every slot in it is still live, unless its block entity is `out`.
    freed: bool,
    /// Positions whose block entity `take` took and `put` has not brought
    /// back.
    out: FxHashMap<Pos, u64>,
    /// Positions a loaded chunk gave a new block entity over an old one: at
    /// the next tick their slot stays only if their block state ticks
    /// (`updateBlockEntityTicker`: rebind, or remove without a ticker).
    replaced: FxHashSet<Pos>,
    /// `Ticking` by block state id; empty until the game builds it.
    states: Vec<Ticking>,/// State-aware native factory ownership. Shared across level snapshots;
    /// an empty slice means the declarations have not yet been compiled.
    stateful_entities: Arc<[bool]>,    /// The positions of the mod block entities (`BlockEntity::Mod`) by chunk,
    /// for the renderer: the mesher lists block entities by their template,
    /// which leaves out a mod block whose template has none. One taken out
    /// (`take`) stays listed until `put` or `remove`.
    mods: FxHashMap<(i32, i32), FxHashSet<Pos>>,
    /// The positions of the block entities that draw a bounding box (structure and test
    /// instance blocks, `shows_box`), so a frame looks at them without scanning the map.
    /// Like `mods`, one taken out stays listed until `put` or `remove`.
    boxes: FxHashSet<Pos>,
    /// Positions by the kind a frame or a tick looks up (`listed`): a beacon's beam, a vault's
    /// item, a campfire, a brushable block, a shelf, and the spawners and potent sulfur a
    /// client ticks. One taken out stays in the set until `put` or `remove`; `listed` hides it
    /// while it is out of the map.
    listed: [FxHashSet<Pos>; LISTED_KINDS],
    /// Every block entity's position by chunk, including one taken out only until `take`
    /// unindexes it. Unloading a chunk reads this instead of the whole map.
    by_chunk: FxHashMap<(i32, i32), FxHashSet<Pos>>,
    /// The signs a player's editor is on (`SignBlockEntity.playerWhoMayEdit`
    /// set), so the level's tick checks those alone instead of every sign.
    sign_editors: FxHashSet<Pos>,
    asleep: FxHashSet<Pos>,
    /// The block entities moving pistons carry, by the moving piston's
    /// position (`carry`): out of the map until they land.
    carried: FxHashMap<Pos, carry::Carried>,
    /// The block entity a player's break just removed, kept readable while that break's
    /// `block_drops` listeners run (`hold_broken`), and dropped after (`release_broken`).
    broken: Option<Broken>,
    /// The API snapshots a client's mod block entities are ticked with
    /// (`take_client_snapshots`), by position.
    client_snapshots: FxHashMap<Pos, ClientSnapshot>,
}

/// A removed block entity held for the `block_drops` event of its break: its position, the
/// identity it had and its saved data, read before the removal took its `CustomName` away.
pub struct Broken {
    pub pos: Pos,
    pub generation: u64,
    pub data: horizon_core::nbt::Compound,
}

/// A client copy of a mod block entity as its client tick shows it to mods: made once for each
/// revision of the copy (an update merged into it is a new one), not once a tick.
pub struct ClientSnapshot {}

/// Storage-owned identity passed to attachment residency observers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct BlockEntityIdentity {
    pub position: [i32; 3],
    pub generation: u64,
}

fn identity(pos: Pos, generations: &FxHashMap<Pos, u64>) -> BlockEntityIdentity {
    BlockEntityIdentity {
        position: [pos.0, pos.1, pos.2],
        generation: generations[&pos],
    }
}


impl Drop for BlockEntities {
    fn drop(&mut self) {
        self.publish_retirement();
    }
}

/// A structure or test instance block: the block entities whose box the frame may draw.
fn shows_box(entity: &BlockEntity) -> bool {
    matches!(
        entity,
        BlockEntity::StructureBlock(_) | BlockEntity::TestInstanceBlock(_)
    )
}

/// The block entities a frame or a tick looks at by kind, apart from the mesher's lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Listed {
    Beacon,
    Vault,
    Campfire,
    Brushable,
    Shelf,
    /// A client ticks spawners, trial spawners and vaults near the player, and potent sulfur
    /// everywhere it knows.
    Spawner,
    TrialSpawner,
    PotentSulfur,
}

const LISTED_KINDS: usize = 8;

fn listed_kind(entity: &BlockEntity) -> Option<Listed> {
    match entity {
        BlockEntity::Beacon(_) => Some(Listed::Beacon),
        BlockEntity::Vault(_) => Some(Listed::Vault),
        BlockEntity::Campfire(_) => Some(Listed::Campfire),
        BlockEntity::Brushable(_) => Some(Listed::Brushable),
        BlockEntity::Shelf(_) => Some(Listed::Shelf),
        BlockEntity::Spawner(_) => Some(Listed::Spawner),
        BlockEntity::TrialSpawner(_) => Some(Listed::TrialSpawner),
        BlockEntity::PotentSulfur(_) => Some(Listed::PotentSulfur),
        _ => None,
    }
}

/// Keeps `pos` listed under the kind of `entity` alone (none: under no kind).
fn note_listed(drawn: &mut [FxHashSet<Pos>; LISTED_KINDS], pos: Pos, entity: Option<&BlockEntity>) {
    let kind = entity.and_then(listed_kind);
    for (index, set) in drawn.iter_mut().enumerate() {
        if kind.is_some_and(|kind| kind as usize == index) {
            set.insert(pos);
        } else {
            set.remove(&pos);
        }
    }
}

fn index_chunk(by_chunk: &mut FxHashMap<(i32, i32), FxHashSet<Pos>>, pos: Pos) {
    by_chunk.entry(chunk_of(pos)).or_default().insert(pos);
}

fn unindex_chunk(by_chunk: &mut FxHashMap<(i32, i32), FxHashSet<Pos>>, pos: &Pos) {
    let chunk = chunk_of(*pos);
    if let Some(set) = by_chunk.get_mut(&chunk) {
        set.remove(pos);
        if set.is_empty() {
            by_chunk.remove(&chunk);
        }
    }
}

fn forget_listed(drawn: &mut [FxHashSet<Pos>; LISTED_KINDS], pos: &Pos) {
    for set in drawn.iter_mut() {
        set.remove(pos);
    }
}

fn chunk_of((x, _, z): Pos) -> (i32, i32) {
    (x >> 4, z >> 4)
}

/// Takes `pos` out of its chunk's mod block entity set.
fn forget_mod(mods: &mut FxHashMap<(i32, i32), FxHashSet<Pos>>, pos: Pos) {
    let chunk = chunk_of(pos);
    if let Some(set) = mods.get_mut(&chunk) {
        set.remove(&pos);
        if set.is_empty() {
            mods.remove(&chunk);
        }
    }
}

impl std::ops::Deref for BlockEntities {
    type Target = HashMap<Pos, BlockEntity>;
    fn deref(&self) -> &Self::Target {
        &self.map
    }
}

impl<'a> IntoIterator for &'a BlockEntities {
    type Item = (&'a Pos, &'a BlockEntity);
    type IntoIter = std::collections::hash_map::Iter<'a, Pos, BlockEntity>;
    fn into_iter(self) -> Self::IntoIter {
        self.map.iter()
    }
}

/// `Vec3i.hashCode`.
fn java_hash((x, y, z): Pos) -> i32 {
    y.wrapping_add(z.wrapping_mul(31))
        .wrapping_mul(31)
        .wrapping_add(x)
}
/// `HashCommon.mix`.
fn mix(x: i32) -> usize {
    let h = (x as u32).wrapping_mul(0x9E37_79B9);
    (h ^ (h >> 16)) as usize
}

/// The order `values().forEach` visits a `new Object2ObjectOpenHashMap<>()`
/// keyed by these positions, put in this order (repeats keep their first
/// slot): open addressing with linear probing, a rehash to twice the size
/// past three quarters full, and iteration from the last slot down.
pub fn chunk_map_order(positions: impl IntoIterator<Item = Pos>) -> Vec<Pos> {
    // HashCommon.arraySize(16, .75f), and maxFill of each size.
    let mut slots: Vec<Option<Pos>> = vec![None; 32];
    let max_fill = |n: usize| (n * 3).div_ceil(4).min(n - 1);
    let mut size = 0;
    for p in positions {
        let mask = slots.len() - 1;
        let mut at = mix(java_hash(p)) & mask;
        let mut known = false;
        while let Some(k) = slots[at] {
            if k == p {
                known = true;
                break;
            }
            at = (at + 1) & mask;
        }
        if known {
            continue;
        }
        slots[at] = Some(p);
        // `if (size++ >= maxFill) rehash(arraySize(size + 1, f))`, which
        // puts the keys back from the last slot down.
        size += 1;
        if size > max_fill(slots.len()) {
            let n = ((size + 1) * 4).div_ceil(3).next_power_of_two();
            let old = std::mem::replace(&mut slots, vec![None; n]);
            for k in old.into_iter().rev().flatten() {
                let mut at = mix(java_hash(k)) & (n - 1);
                while slots[at].is_some() {
                    at = (at + 1) & (n - 1);
                }
                slots[at] = Some(k);
            }
        }
    }
    slots.into_iter().rev().flatten().collect()
}

impl BlockEntities {
    /// Gameplay supplies its subscription decision; storage knows no hook identifiers.
    pub fn set_on_load_enabled(&mut self, enabled: bool) {
        self.on_load_enabled = enabled;
        if !enabled {
            self.on_load.clear();
        }
    }
    /// Whether any attachment notices remain, including possibly retired identities.
    pub fn has_pending_on_load(&self) -> bool {
        !self.on_load.is_empty()
    }
    /// Number of queued notices, including retired entries, at a drain boundary.
    pub fn pending_on_load_len(&self) -> usize {
        self.on_load.len()
    }
    /// Take a live notice from a bounded prefix, counting stale entries too.
    pub fn next_on_load_in_batch(&mut self, remaining: &mut usize) -> Option<(Pos, u64)> {
        while *remaining > 0 {
            *remaining -= 1;
            let (pos, generation) = self.on_load.pop_front()?;
            if self.generation(&pos) == Some(generation)
                && false
            {
                return Some((pos, generation));
            }
        }
        None
    }
    /// Take the next newly attached mod identity. Removed or replaced entries
    /// are stale and skipped; payload rewrites retain the queued generation.
    pub fn next_on_load(&mut self) -> Option<(Pos, u64)> {
        let mut remaining = self.on_load.len();
        self.next_on_load_in_batch(&mut remaining)
    }

    /// Runtime identity, retained while an entity is temporarily taken for its tick.
    pub fn generation(&self, pos: &Pos) -> Option<u64> {
        self.generations.get(pos).copied()
    }
    /// Restore the identity captured with an existing entity by a transaction.
    pub fn restore_generation(&mut self, pos: Pos, generation: u64) {
        assert!(self.map.contains_key(&pos));
        if self.generation(&pos) != Some(generation) {
            self.generations.insert(pos, generation);
        }
    }
    fn new_generation(&mut self, pos: Pos) {
        self.generations.insert(pos, next_token());
    }


    /// The client tick snapshots, lent for one pass over the tickers: give them back with
    /// `keep_client_snapshots`.
    pub fn take_client_snapshots(&mut self) -> FxHashMap<Pos, ClientSnapshot> {
        std::mem::take(&mut self.client_snapshots)
    }
    /// Takes back what `take_client_snapshots` lent, without the positions that no longer tick.
    pub fn keep_client_snapshots(&mut self, mut snapshots: FxHashMap<Pos, ClientSnapshot>) {
        snapshots.retain(|pos, _| self.live.contains_key(pos));
        self.client_snapshots = snapshots;
    }
    /// Whether the ticking table has been built.
    pub fn has_states(&self) -> bool {
        !self.states.is_empty()
    }
    pub fn set_states(&mut self, states: Vec<Ticking>) {
        self.states = states;
    }
    pub fn ticking(&self, state: u32) -> Ticking {
        self.states.get(state as usize).copied().unwrap_or_default()
    }
    pub fn set_stateful_entities(&mut self, states: Vec<bool>) {
        self.stateful_entities = states.into();
    }
    pub fn has_stateful_entity_table(&self) -> bool {
        !self.stateful_entities.is_empty()
    }
    pub fn stateful_entities(&self) -> Arc<[bool]> {
        self.stateful_entities.clone()
    }
    pub fn stateful_entity(&self, state: u32) -> bool {
        self.stateful_entities
            .get(state as usize)
            .copied()
            .unwrap_or(false)
    }

    fn next(&mut self) -> u32 {
        let n = self.next_slot;
        self.next_slot = n.wrapping_add(1);
        n
    }
    /// A ticker may be made for `pos` now (`updateBlockEntityTicker`).
    fn add(&mut self, pos: Pos) {
        let token = self.next();
        self.pending.insert(pos, token);
        self.added.push((pos, token));
    }
    /// `removeBlockEntityTicker`: no slot, made or pending.
    fn cancel(&mut self, pos: &Pos) {
        self.asleep.remove(pos);
        self.freed |= self.live.remove(pos).is_some();
        self.pending.remove(pos);
    }

    /// Retires the map of the block entity leaving `pos`: at once, or, while a transaction
    /// defers retirement, kept bound until it closes.
    fn retire(&mut self, pos: Pos) {
    }
    /// From now until `publish_retirement`, a block entity that goes keeps its map bound to
    /// its index entries: the root transaction opened, and only its commit publishes what it
    /// removed.
    pub fn defer_retirement(&mut self) {
    }
    /// Whether a transaction is open: between `defer_retirement` and `publish_retirement`.
    pub fn defers_retirement(&self) -> bool {
        false
    }
    /// Retires every map kept since `defer_retirement`, and stops deferring: the root
    /// transaction closed. What an abort did not put back goes: those of block entities a
    /// committed transaction removed, and those of block entities an aborted one made.
    pub fn publish_retirement(&mut self) {
    }
    /// Gives the block entity of `generation` that stands at `pos` again the map it went with
    /// in this transaction, as it was then, still bound: a removed block entity a later write
    /// in the same transaction put back.
    pub fn reattach_retired(&mut self, pos: Pos, generation: u64) {
    }
    /// Whether any mod block entity is in the map: until one is, a block change
    /// never looks one up (`LevelAccess::has_mod_block_entity`).
    pub fn has_mods(&self) -> bool {
        !self.mods.is_empty()
    }
    /// The structure and test instance blocks (a frame draws their boxes), in no order.
    pub fn boxes(&self) -> impl Iterator<Item = (&Pos, &BlockEntity)> {
        self.boxes
            .iter()
            .filter_map(|pos| Some((pos, self.map.get(pos)?)))
    }
    /// The block entities of `kind`, in no order, without scanning the map. One `take` took out
    /// stays in the set until `put` or `remove`, and is skipped here while it is out.
    pub fn listed(&self, kind: Listed) -> impl Iterator<Item = (&Pos, &BlockEntity)> {
        self.listed[kind as usize]
            .iter()
            .filter_map(|pos| Some((pos, self.map.get(pos)?)))
    }
    /// Keeps `pos` listed in `boxes` exactly while `entity` shows a box.
    fn note_box(&mut self, pos: Pos, entity: Option<&BlockEntity>) {
        if entity.is_some_and(shows_box) {
            self.boxes.insert(pos);
        } else {
            self.boxes.remove(&pos);
        }
    }
    /// The mod block entities within `radius` blocks of `x`, `z` (by chunk, so
    /// far ones are skipped whole; the caller checks each distance).
    pub fn mods_near(&self, x: f64, z: f64, radius: f64) -> impl Iterator<Item = &Pos> {
        let far = move |c: i32, at: f64| {
            let (lo, hi) = (f64::from(c * 16), f64::from(c * 16 + 16));
            at < lo - radius || at > hi + radius
        };
        self.mods
            .iter()
            .filter(move |((cx, cz), _)| !far(*cx, x) && !far(*cz, z))
            .flat_map(|(_, set)| set)
    }
    /// The mod block entities of chunk `chunk`, in no order; one taken out is
    /// listed too.
    pub fn mods_in(&self, chunk: (i32, i32)) -> impl Iterator<Item = &Pos> {
        self.mods.get(&chunk).into_iter().flatten()
    }
    /// The positions of the block entities in `chunk` (`LevelChunk.getBlockEntities`), in no
    /// order; one taken out for its tick is not among them until `put`.
    pub fn in_chunk(&self, chunk: (i32, i32)) -> impl Iterator<Item = &Pos> {
        self.by_chunk.get(&chunk).into_iter().flatten()
    }
    /// `SignBlockEntity.playerWhoMayEdit` of the sign at `pos`.
    pub fn sign_editor(&self, pos: &Pos) -> Option<u128> {
        match self.map.get(pos) {
            Some(BlockEntity::Sign(sign)) => sign.allowed_editor,
            _ => None,
        }
    }
    /// `SignBlockEntity.setAllowedPlayerEditor`: the one player whose editor
    /// is on the sign at `pos`, or `None` for nobody's.
    pub fn set_sign_editor(&mut self, pos: Pos, who: Option<u128>) {
        let Some(BlockEntity::Sign(sign)) = self.map.get_mut(&pos) else {
            return;
        };
        sign.allowed_editor = who;
        if who.is_some() {
            self.sign_editors.insert(pos);
        } else {
            self.sign_editors.remove(&pos);
        }
    }
    /// The positions of the signs a player's editor is on, in no order.
    pub fn sign_editors(&self) -> impl Iterator<Item = &Pos> {
        self.sign_editors.iter()
    }
    /// Drops `pos` from the signs an editor is on, for a sign that is gone or
    /// no longer holds one. `set_sign_editor` keeps the list itself.
    pub fn forget_sign_editor(&mut self, pos: Pos) {
        self.sign_editors.remove(&pos);
    }
    /// Keeps `pos` listed in `sign_editors` exactly while `entity` holds an
    /// editor, as `note_box` does for the boxes.
    fn note_sign_editor(&mut self, pos: Pos, entity: &BlockEntity) {
        if matches!(entity, BlockEntity::Sign(sign) if sign.allowed_editor.is_some()) {
            self.sign_editors.insert(pos);
        } else {
            self.sign_editors.remove(&pos);
        }
    }
    /// `LevelChunk.getBlockEntity`: the block entity at `pos`, if any.
    pub fn get(&self, pos: &Pos) -> Option<&BlockEntity> {
        self.map.get(pos)
    }
    pub fn get_mut(&mut self, pos: &Pos) -> Option<&mut BlockEntity> {
        self.asleep.remove(pos);
        self.map.get_mut(pos)
    }
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&Pos, &mut BlockEntity)> {
        self.asleep.clear();
        self.map.iter_mut()
    }
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut BlockEntity> {
        self.asleep.clear();
        self.map.values_mut()
    }
    pub fn sleep(&mut self, pos: Pos) {
        if self.map.contains_key(&pos) {
            self.asleep.insert(pos);
        }
    }
    pub fn is_asleep(&self, pos: &Pos) -> bool {
        self.asleep.contains(pos)
    }
    fn replace_taken(&mut self, pos: Pos) {
        if self.out.remove(&pos).is_some() {
            self.replaced.insert(pos);
        }
    }
    /// `LevelChunk.setBlockEntity` (+ `addAndRegisterBlockEntity`): a new
    /// block entity may get a slot; one replacing another keeps its slot.
    /// One of the same type rewrites it in place (a jukebox taking a disc,
    /// `/data merge block`), where vanilla keeps the object, so its
    /// attachments stay; any other replacement starts them over.
    pub fn insert(&mut self, pos: Pos, entity: BlockEntity) -> Option<BlockEntity> {
        self.insert_impl(pos, entity, false)
    }
    /// Attach a different instance, even when its type matches the old instance.
    /// Payload-only writes use `insert` and retain their identity instead.
    pub fn replace(&mut self, pos: Pos, entity: BlockEntity) -> Option<BlockEntity> {
        self.insert_impl(pos, entity, true)
    }
    fn insert_impl(
        &mut self,
        pos: Pos,
        entity: BlockEntity,
        replace_identity: bool,
    ) -> Option<BlockEntity> {
        let new_is_mod = false;
        let same_type = |old: &BlockEntity| match (old, &entity) {
            (old, new) => std::mem::discriminant(old) == std::mem::discriminant(new),
        };
        let new_identity = replace_identity || !self.map.get(&pos).is_some_and(same_type);
        if new_identity {
            self.new_generation(pos);
        }
        self.replace_taken(pos);
        let new_box = shows_box(&entity);
        self.note_sign_editor(pos, &entity);
        note_listed(&mut self.listed, pos, Some(&entity));
        self.asleep.remove(&pos);
        index_chunk(&mut self.by_chunk, pos);
        let old = self.map.insert(pos, entity);
        if self.on_load_enabled && new_identity && new_is_mod {
            self.on_load.push_back((pos, self.generations[&pos]));
        }
        if old.is_none() {
            self.add(pos);
        }
        if new_box {
            self.boxes.insert(pos);
        } else {
            self.boxes.remove(&pos);
        }
        if new_is_mod {
            self.mods.entry(chunk_of(pos)).or_default().insert(pos);
        } else {
            forget_mod(&mut self.mods, pos);
        }
        old
    }
    /// The block entity at `pos`, made by `make` when there is none.
    pub fn get_or_insert_with(
        &mut self,
        pos: Pos,
        make: impl FnOnce() -> BlockEntity,
    ) -> &mut BlockEntity {
        let absent = !self.map.contains_key(&pos);
        if absent {
            self.replace_taken(pos);
            self.new_generation(pos);
            forget_mod(&mut self.mods, pos);
            self.boxes.remove(&pos);
            self.add(pos);
        }
        self.asleep.remove(&pos);
        index_chunk(&mut self.by_chunk, pos);
        let entity = self.map.entry(pos).or_insert_with(make);
        if absent && false {
            if self.on_load_enabled {
                self.on_load.push_back((pos, self.generations[&pos]));
            }
            self.mods.entry(chunk_of(pos)).or_default().insert(pos);
        }
        if absent && shows_box(entity) {
            self.boxes.insert(pos);
        }
        if absent {
            note_listed(&mut self.listed, pos, Some(entity));
        }
        entity
    }
    /// Keep `data`, the saved data (with its `CustomName`) of the block entity just removed from
    /// `pos`, where it had `generation`, readable through [`Self::broken`] until
    /// [`Self::release_broken`]. The map and the generations do not know it: a handle to it
    /// answers reads, and nothing else. One slot: a second hold replaces the first. Listeners of
    /// the event it serves read through a shared host and cannot start another break, so the two
    /// do not nest today; a hook that could would hold per break instead.
    pub fn hold_broken(&mut self, pos: Pos, generation: u64, data: horizon_core::nbt::Compound) {
        self.broken = Some(Broken {
            pos,
            generation,
            data,
        });
    }
    /// The data [`Self::hold_broken`] holds, if it is that of the block entity at `pos` with
    /// `generation`.
    pub fn broken(&self, pos: &Pos, generation: u64) -> Option<&Broken> {
        self.broken
            .as_ref()
            .filter(|held| held.pos == *pos && held.generation == generation)
    }
    /// The generation of the block entity held at `pos`, if any.
    pub fn broken_generation(&self, pos: &Pos) -> Option<u64> {
        self.broken
            .as_ref()
            .filter(|held| held.pos == *pos)
            .map(|held| held.generation)
    }
    /// Drop the held data: every handle to it is stale from now on.
    pub fn release_broken(&mut self) {
        self.broken = None;
    }
    /// `LevelChunk.removeBlockEntity`: the block entity and its slot.
    pub fn remove(&mut self, pos: &Pos) -> Option<BlockEntity> {
        self.out.remove(pos);
        self.generations.remove(pos);
        self.asleep.remove(pos);
        self.cancel(pos);
        let removed = self.map.remove(pos);
        unindex_chunk(&mut self.by_chunk, pos);
        forget_mod(&mut self.mods, *pos);
        self.boxes.remove(pos);
        forget_listed(&mut self.listed, pos);
        self.sign_editors.remove(pos);
        removed
    }
    /// The block entity at `pos`, out of the map for a while: its slot stays
    /// for `put` to bring it back (a tick working on it).
    pub fn take(&mut self, pos: &Pos) -> Option<(BlockEntityLease, BlockEntity)> {
        self.asleep.remove(pos);
        let entity = self.map.remove(pos)?;
        unindex_chunk(&mut self.by_chunk, pos);
        let token = next_token();
        self.out.insert(*pos, token);
        let generation = self.generations[pos];
        Some((
            BlockEntityLease {
                pos: *pos,
                token,
                generation,
            },
            entity,
        ))
    }
    /// Re-key a live lease when an intentional state change keeps the instance.
    /// Ordinary removal still revokes it; a retired or foreign lease cannot re-key.
    pub fn rekey(&mut self, pos: &Pos, lease: &mut BlockEntityLease) -> bool {
        if lease.pos != *pos
            || self.out.get(pos) != Some(&lease.token)
            || self.generation(pos) != Some(lease.generation)
        {
            return false;
        }
        lease.token = next_token();
        self.out.insert(*pos, lease.token);
        true
    }
    /// Return the taken instance in its slot; false if its lease was retired.
    pub fn put(&mut self, lease: BlockEntityLease, entity: BlockEntity) -> bool {
        let pos = lease.pos;
        if self.out.get(&pos) != Some(&lease.token)
            || self.generation(&pos) != Some(lease.generation)
        {
            return false;
        }
        self.out.remove(&pos);
        self.asleep.remove(&pos);
        if false {
            self.mods.entry(chunk_of(pos)).or_default().insert(pos);
        }
        self.note_box(pos, Some(&entity));
        note_listed(&mut self.listed, pos, Some(&entity));
        self.note_sign_editor(pos, &entity);
        index_chunk(&mut self.by_chunk, pos);
        if self.map.insert(pos, entity).is_none()
            && !self.live.contains_key(&pos)
            && !self.pending.contains_key(&pos)
        {
            self.add(pos);
        }
        true
    }
    fn retire_taken(&mut self, mut gone: impl FnMut(&Pos) -> bool) {
        let mut out = std::mem::take(&mut self.out);
        out.retain(|pos, _| {
            if gone(pos) {
                self.remove(pos);
                false
            } else {
                true
            }
        });
        self.out = out;
    }
    /// [`Self::retain`] for unloading chunks: removes the block entities at
    /// the positions `gone` names and changes none it keeps, so those that
    /// sleep stay asleep (a moving player unloads chunks most ticks).
    pub fn unload(&mut self, mut gone: impl FnMut(&Pos) -> bool) {
        self.carried.retain(|p, _| !gone(p));
        let asleep = std::mem::take(&mut self.asleep);
        self.retain(|p, _| !gone(p));
        self.retire_taken(gone);
        self.asleep = asleep;
        let map = &self.map;
        self.asleep.retain(|p| map.contains_key(p));
    }
    /// Removes the block entities `keep` refuses, and their slots
    /// (`clearAllBlockEntities` of unloaded chunks).
    pub fn retain(&mut self, mut keep: impl FnMut(&Pos, &mut BlockEntity) -> bool) {
        let (live, pending, freed) = (&mut self.live, &mut self.pending, &mut self.freed);
        let (mods, boxes, listed) = (&mut self.mods, &mut self.boxes, &mut self.listed);
        let by_chunk = &mut self.by_chunk;
        let generations = &mut self.generations;
        let sign_editors = &mut self.sign_editors;
        self.asleep.clear();
        self.map.retain(|p, e| {
            let kept = keep(p, e);
            if !kept {
                generations.remove(p);
                *freed |= live.remove(p).is_some();
                pending.remove(p);
                forget_mod(mods, *p);
                boxes.remove(p);
                forget_listed(listed, p);
                unindex_chunk(by_chunk, p);
                sign_editors.remove(p);
            }
            kept
        });
    }
    /// [`Self::unload`] of the block entities in `chunks`, reading theirs alone ([`Self::by_chunk`])
    /// rather than every block entity of the world. The same block entities go, with the same of
    /// what they hold, and those that sleep elsewhere stay asleep.
    pub fn unload_chunks(&mut self, chunks: &[(i32, i32)]) {
        let set: FxHashSet<(i32, i32)> = chunks.iter().copied().collect();
        let gone = |p: &Pos| set.contains(&chunk_of(*p));
        self.carried.retain(|p, _| !gone(p));
        let positions: Vec<Pos> = set
            .iter()
            .filter_map(|chunk| self.by_chunk.get(chunk))
            .flatten()
            .copied()
            .collect();
        for p in &positions {
            if self.map.remove(p).is_none() {
                continue;
            }
            self.generations.remove(p);
            self.freed |= self.live.remove(p).is_some();
            self.pending.remove(p);
            forget_mod(&mut self.mods, *p);
            self.boxes.remove(p);
            forget_listed(&mut self.listed, p);
            unindex_chunk(&mut self.by_chunk, p);
            self.sign_editors.remove(p);
            self.asleep.remove(p);
        }
        self.retire_taken(gone);
    }
    pub fn clear(&mut self) {
        self.generations.clear();
        self.on_load.clear();
        self.map.clear();
        self.by_chunk.clear();
        self.mods.clear();
        self.boxes.clear();
        for set in &mut self.listed {
            set.clear();
        }
        self.sign_editors.clear();
        self.asleep.clear();
        self.tickers = Arc::default();
        self.live.clear();
        self.added.clear();
        self.pending.clear();
        self.out.clear();
        self.replaced.clear();
        self.carried.clear();
        self.client_snapshots.clear();
        self.freed = false;
    }
    /// A loaded or generated chunk's block entities, in the order the chunk
    /// lists them; they tick in its block entity map's order
    /// (`registerAllBlockEntitiesAfterLevelLoad`). One replacing an entity
    /// already at its position (`LevelChunk.setBlockEntity`) drops that one;
    /// its slot is decided by the block state at the next tick.
    pub fn load_chunk(&mut self, entities: Vec<(Pos, BlockEntity)>) {
        let order = chunk_map_order(entities.iter().map(|(p, _)| *p));
        for (p, e) in entities {
            self.replace_taken(p);
            self.new_generation(p);
            self.asleep.remove(&p);
            let new_is_mod = false;
            self.note_box(p, Some(&e));
            note_listed(&mut self.listed, p, Some(&e));
            self.note_sign_editor(p, &e);
            index_chunk(&mut self.by_chunk, p);
            let old = self.map.insert(p, e);
            if old.is_some() {
                self.replaced.insert(p);
            }
            if new_is_mod {
                self.mods.entry(chunk_of(p)).or_default().insert(p);
            } else {
                forget_mod(&mut self.mods, p);
            }
        }
        for p in order {
            if self.on_load_enabled && false {
                self.on_load.push_back((p, self.generations[&p]));
            }
            self.add(p);
        }
    }
    /// `LevelChunk.setBlockState` on a block entity's position, from state
    /// `old` to `new`: a ticker made, kept or dropped.
    pub fn state_changed(&mut self, pos: Pos, old: u32, new: u32) {
        self.asleep.remove(&pos);
        if self.states.is_empty() || !self.map.contains_key(&pos) {
            return;
        }
        match self.ticking(new) {
            // A new ticker, unless one was made or is pending (rebound).
            Ticking::Yes => {
                if self.ticking(old) != Ticking::Yes && !self.live.contains_key(&pos) {
                    self.add(pos);
                }
            }
            Ticking::Idle => self.cancel(&pos),
            // Another block: the game removes or replaces the block entity.
            Ticking::No => {}
        }
    }
    /// The start of `Level.tickBlockEntities`: pending slots join the list
    /// and freed ones leave it. `state` reads the block state at a position.
    /// Returns the slots to tick, in order; check each with `is_live` before
    /// ticking it, as the ticks before it can free it.
    pub fn tickers(&mut self, state: impl Fn(Pos) -> u32) -> Arc<Vec<(Pos, u32)>> {
        for (pos, token) in std::mem::take(&mut self.added) {
            if self.pending.get(&pos) != Some(&token) {
                continue;
            }
            self.pending.remove(&pos);
            if self.live.contains_key(&pos)
                || !self.map.contains_key(&pos)
                || self.ticking(state(pos)) != Ticking::Yes
            {
                continue;
            }
            let slot = self.next();
            self.live.insert(pos, slot);
            Arc::make_mut(&mut self.tickers).push((pos, slot));
        }
        for pos in std::mem::take(&mut self.replaced) {
            if self.live.contains_key(&pos) && self.ticking(state(pos)) != Ticking::Yes {
                self.cancel(&pos);
            }
        }
        if !std::mem::take(&mut self.freed) && self.out.is_empty() {
            return Arc::clone(&self.tickers);
        }
        self.retire_taken(|_| true);
        self.freed = false;
        let (live, map) = (&mut self.live, &self.map);
        Arc::make_mut(&mut self.tickers).retain(|(p, slot)| {
            let mine = live.get(p) == Some(slot);
            if mine && !map.contains_key(p) {
                live.remove(p);
            }
            mine && map.contains_key(p)
        });
        Arc::clone(&self.tickers)
    }
    /// The ticker list as it stands, without promoting pending slots.
    pub fn ticker_slots(&self) -> Arc<Vec<(Pos, u32)>> {
        Arc::clone(&self.tickers)
    }
    /// Whether a slot from `tickers` still ticks.
    pub fn is_live(&self, (pos, slot): (Pos, u32)) -> bool {
        self.live.get(&pos) == Some(&slot) && self.map.contains_key(&pos)
    }
    /// Frees the slot at `pos` (the block entity stays).
    pub fn forget(&mut self, pos: &Pos) {
        self.cancel(pos);
    }
}




