//! Resident chunk membership, with a token for consumers that cache its keys.
use crate::{
    chunk::Chunk,
    fast_hash::{FxHashMap, FxHashSet},
};
use std::{
    collections::{
        VecDeque,
        hash_map::{Iter, ValuesMut},
    },
    ops::Deref,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};


type Key = (i32, i32);
type Columns = FxHashMap<Key, Arc<Chunk>>;
static NEXT_REVISION: AtomicU64 = AtomicU64::new(1);
fn next_revision() -> u64 {
    NEXT_REVISION.fetch_add(1, Ordering::Relaxed)
}

/// The membership changes one map keeps for the consumers that read them
/// every tick (`TickChunkMembership.synchronize`); older ones are dropped.
const MAX_RECENT_DELTAS: usize = 256;

/// One membership change of one `ChunkMap`, stamped with the revision it
/// brought the map to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChunkMembershipDelta {
    Added(Key),
    Removed(Key),
    Reset,
}

/// Read access behaves like a map. Structural writes are explicit so edits to
/// blocks or light don't invalidate consumers of the loaded chunk key set.
/// Tokens are process-wide: replacing the entire map also invalidates caches.
pub struct ChunkMap {
    columns: Columns,
    revision: u64,
    /// The oldest revision `recent_deltas` still answers for: the revision of
    /// the change it last dropped, or the revision that cleared them. A
    /// consumer holding an older one rebuilds, it does not replay.
    min_valid_revision: u64,
    /// This map's recent membership changes, in revision order.
    /// Revisions are process-wide: the numbers missing in between belong to
    /// the other maps (a server thread's, another dimension's), which say
    /// nothing about this one's keys.
    recent_deltas: VecDeque<(u64, ChunkMembershipDelta)>,
    /// The resident chunks generation has not placed its animals in yet.
    fresh: FxHashSet<Key>,    /// For each column, how many of the 3×3 columns around it (itself
    /// included) are resident; absent when none is. `surrounded` asks one
    /// map instead of nine every tick, for every ticking chunk.
    near: FxHashMap<Key, u8>,
    /// Which map this is, for the consumers of `touched_since` (a replaced or swapped map).
    identity: u64,
    /// The columns a writer was handed, inserted or removed, in order: a consumer that keeps a
    /// per-column answer re-reads only these (`touched_since`).
    touched: VecDeque<Key>,
    /// The sequence number of `touched[0]`.
    touched_base: u64,
    /// The sequence number from which every column may have changed (`values_mut`, `clear`).
    touched_all: u64,
}
/// The columns `ChunkMap::touched_since` remembers; an older cursor rebuilds.
const MAX_TOUCHED: usize = 8192;
impl Default for ChunkMap {
    fn default() -> Self {
        let revision = next_revision();
        Self {
            columns: Columns::default(),
            revision,
            min_valid_revision: revision,
            recent_deltas: VecDeque::new(),
            fresh: FxHashSet::default(),
            near: FxHashMap::default(),
            identity: next_revision(),
            touched: VecDeque::new(),
            touched_base: 0,
            touched_all: 0,
        }
    }
}

impl Drop for ChunkMap {
    fn drop(&mut self) {
    }
}

impl ChunkMap {



    pub fn membership_revision(&self) -> u64 {
        self.revision
    }
    /// The map's identity and the sequence number the next touch takes: a consumer that
    /// has read every column now keeps them for `touched_since`.
    pub fn touch_cursor(&self) -> (u64, u64) {
        (self.identity, self.touched_base + self.touched.len() as u64)
    }
    /// The columns touched (handed out mutably, inserted, removed) since `cursor`, a
    /// `touch_cursor` of this map; `None` when everything may have changed since (another
    /// map, `values_mut`, `clear`, or a cursor older than what is kept): read it all again.
    /// A column not listed is the one the consumer read last.
    pub fn touched_since(&self, cursor: (u64, u64)) -> Option<impl Iterator<Item = &Key> + '_> {
        let (identity, seq) = cursor;
        if identity != self.identity || seq < self.touched_all || seq < self.touched_base {
            return None;
        }
        Some(self.touched.iter().skip((seq - self.touched_base) as usize))
    }
    fn touch(&mut self, key: Key) {
        if self.touched.len() >= MAX_TOUCHED {
            self.touched.pop_front();
            self.touched_base += 1;
        }
        self.touched.push_back(key);
    }
    fn touch_all(&mut self) {
        // One more than the entries dropped: the cursor moves even for a consumer that had
        // read every entry (its sequence number is then the old end of the log).
        self.touched_base += self.touched.len() as u64 + 1;
        self.touched.clear();
        self.touched_all = self.touched_base;
    }
    fn record_delta(&mut self, delta: ChunkMembershipDelta) {
        if self.recent_deltas.len() >= MAX_RECENT_DELTAS
            && let Some((evicted, _)) = self.recent_deltas.pop_front()
        {
            self.min_valid_revision = evicted;
        }
        self.recent_deltas.push_back((self.revision, delta));
    }
    /// This map's membership changes since `last_revision`, one of its own
    /// earlier `membership_revision`s, or `None` when they can no longer be
    /// answered: the consumer's revision predates what the ring kept, the map
    /// was cleared since (a reset rebuilds everything), or the revision is not
    /// this map's, which its own revisions give away: every one of them in the
    /// answered range is the opening revision or a change the ring still
    /// holds, so a revision another map took in between is refused, not
    /// replayed.
    pub fn deltas_since(
        &self,
        last_revision: u64,
    ) -> Option<impl Iterator<Item = (u64, ChunkMembershipDelta)> + '_> {
        if last_revision == 0
            || last_revision < self.min_valid_revision
            || last_revision > self.revision
            || self.recent_deltas.is_empty()
        {
            return None;
        }
        let pos = self
            .recent_deltas
            .partition_point(|(rev, _)| *rev <= last_revision);
        // A revision this map never took: another map's, drawing on the same
        // process-wide counter. Only the opening revision and the ones the
        // ring still holds are this map's.
        let is_own_revision = last_revision == self.min_valid_revision
            || (pos > 0 && self.recent_deltas[pos - 1].0 == last_revision);
        if !is_own_revision {
            return None;
        }
        let slice = self.recent_deltas.range(pos..);
        if slice
            .clone()
            .any(|(_, d)| *d == ChunkMembershipDelta::Reset)
        {
            return None;
        }
        Some(slice.copied())
    }
    pub fn insert(&mut self, key: Key, mut chunk: Arc<Chunk>) -> Option<Arc<Chunk>> {
        self.touch(key);
        let limits = chunk
            .saved
            .as_ref()
            .map(|saved| {
                saved
                    .block_entity_sources
                    .iter()
                    .filter_map(|(pos, tag)| match tag.get("horizon:stack_limit") {
                        Some(horizon_core::nbt::Tag::Int(n)) if (1..=99).contains(n) => {
                            Some((*pos, *n as u32))
                        }
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        if chunk.container_stack_limits != limits {
            Arc::make_mut(&mut chunk).container_stack_limits = limits;
        }
        if chunk.fresh {
            self.fresh.insert(key);
        } else {
            self.fresh.remove(&key);
        }
        let previous = self.columns.insert(key, chunk);
        if previous.is_none() {
            for z in key.1 - 1..=key.1 + 1 {
                for x in key.0 - 1..=key.0 + 1 {
                    *self.near.entry((x, z)).or_insert(0) += 1;
                }
            }
            self.revision = next_revision();
            self.record_delta(ChunkMembershipDelta::Added(key));
        }
        previous
    }
    pub fn remove(&mut self, key: &Key) -> Option<Arc<Chunk>> {
        self.touch(*key);
        self.fresh.remove(key);
        let previous = self.columns.remove(key);
        if previous.is_some() {
            for z in key.1 - 1..=key.1 + 1 {
                for x in key.0 - 1..=key.0 + 1 {
                    if let Some(n) = self.near.get_mut(&(x, z)) {
                        *n -= 1;
                        if *n == 0 {
                            self.near.remove(&(x, z));
                        }
                    }
                }
            }
            self.revision = next_revision();
            self.record_delta(ChunkMembershipDelta::Removed(*key));
        }
        previous
    }
    pub fn clear(&mut self) {
        self.touch_all();
        if !self.columns.is_empty() {
            self.columns.clear();
            self.fresh.clear();
            self.near.clear();
            self.revision = next_revision();
            self.recent_deltas.clear();
            self.recent_deltas
                .push_back((self.revision, ChunkMembershipDelta::Reset));
            self.min_valid_revision = self.revision;
        }
    }
    /// Whether the column at `key` and its eight neighbours are all resident.
    pub fn surrounded(&self, key: Key) -> bool {
        self.near.get(&key) == Some(&9)
    }
    pub fn has_fresh(&self) -> bool {
        !self.fresh.is_empty()
    }
    /// The fresh keys in the chunk map's own iteration order, so the spawns
    /// they drive run in the order a scan of the columns gave.
    pub fn fresh_keys_ordered(&self) -> Vec<Key> {
        if self.fresh.is_empty() {
            Vec::new()
        } else if self.fresh.len() == 1 {
            self.fresh.iter().copied().collect()
        } else {
            self.columns
                .keys()
                .filter(|k| self.fresh.contains(k))
                .copied()
                .collect()
        }
    }
    pub fn remove_fresh(&mut self, key: &Key) {
        self.fresh.remove(key);
    }
    pub fn get_mut(&mut self, key: &Key) -> Option<&mut Arc<Chunk>> {
        self.touch(*key);
        self.columns.get_mut(key)
    }
    pub fn values_mut(&mut self) -> ValuesMut<'_, Key, Arc<Chunk>> {
        self.touch_all();
        self.columns.values_mut()
    }
}
impl Deref for ChunkMap {
    type Target = Columns;
    fn deref(&self) -> &Columns {
        &self.columns
    }
}
impl<'a> IntoIterator for &'a ChunkMap {
    type Item = (&'a Key, &'a Arc<Chunk>);
    type IntoIter = Iter<'a, Key, Arc<Chunk>>;
    fn into_iter(self) -> Self::IntoIter {
        self.columns.iter()
    }
}
impl crate::light::LightWorld for ChunkMap {
    fn section(&self, cx: i32, s: usize, cz: i32) -> Option<&Arc<crate::chunk::Section>> {
        self.columns.section(cx, s, cz)
    }
    fn set_section_light(&mut self, cx: i32, s: usize, cz: i32, light: crate::chunk::Light) {
        self.columns.set_section_light(cx, s, cz, light);
    }
    fn layout(&self) -> crate::chunk::ColumnLayout {
        self.columns.layout()
    }
    fn dynamic_emission(&self, x: i32, y: i32, z: i32) -> Option<u8> {
        self.columns.dynamic_emission(x, y, z)
    }
}

