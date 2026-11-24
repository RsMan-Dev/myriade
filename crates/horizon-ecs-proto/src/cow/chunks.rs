//! The records of the loaded chunks: one `ChunkRec` per chunk that is loaded, in a `Vec` whose slots are reused, so memory follows the loaded
//! chunks and nothing is bounded by the size of the world. A column is named by a `cell`: `slot << 8 | column in the chunk` (`u32::MAX`: none).
use super::*;
use horizon_core::fast_hash::FxHashMap;
use std::sync::atomic::{AtomicU32, Ordering::Relaxed};

/// No slot (a chunk that is not loaded).
pub(crate) const NO_SLOT: u32 = u32::MAX;

/// The entities of the columns of a chunk, built when the first one comes in.
pub(crate) struct Cols {
    pub(crate) cells: [Vec<Entity>; 256],
    /// Parallel to `cells`: the XZ extent of each entity's box (min x, max x, min z, max z) relative to the column's corner, widened by `SLOT_MARGIN`,
    /// as `f32`: `for_near_touching` skips the neighbours whose extent does not touch its own without an ECS lookup. An entity whose box
    /// `reindex` has not seen yet holds an infinite extent (always a candidate).
    pub(crate) slots: [Vec<[f32; 4]>; 256],
    /// The tick that last listed the column as a candidate (`player_near`).
    pub(crate) stamp: [u32; 256],
}

/// The hazard blocks of a chunk, built when the first one is set: per column 0, or 1 + the index in `pool`.
pub(crate) struct HazCols {
    pub(crate) cols: [u32; 256],
    pub(crate) pool: Vec<Vec<(i32, u8)>>,
}

pub struct ChunkRec {
    pub(crate) cx: i32,
    pub(crate) cz: i32,
    pub(crate) live: bool,
    /// The slots of the chunks around, `(dz + 1) * 3 + dx + 1` (the centre is this chunk), `NO_SLOT` when that one is not loaded.
    pub(crate) nbr: [u32; 9],
    /// The entities listed in the chunk.
    pub(crate) len: u32,
    /// A bit per column of each row of the chunk: the columns that hold an entity.
    pub(crate) rows: [u16; 16],
    pub(crate) cols: Option<Box<Cols>>,
    pub(crate) haz: Option<Box<HazCols>>,
    /// The hazard entries of the chunk.
    pub(crate) haz_count: u32,
    /// The zone (`zone.rs`): the players that cover the chunk, their count before this tick's moves, and the epoch that saved it.
    pub(crate) count: u16,
    pub(crate) pre: u16,
    pub(crate) stamp: u32,
}

impl ChunkRec {
    fn empty() -> Self {
        Self { cx: 0, cz: 0, live: false, nbr: [NO_SLOT; 9], len: 0, rows: [0; 16], cols: None, haz: None, haz_count: 0, count: 0, pre: 0, stamp: 0 }
    }
}

/// A direct-mapped memo of the last chunks asked by position (checked against the record, so a stale entry is only a miss).
struct Memo([AtomicU32; 256]);

impl Default for Memo {
    fn default() -> Self {
        Self(std::array::from_fn(|_| AtomicU32::new(NO_SLOT)))
    }
}

#[derive(Default)]
pub struct Chunks {
    pub(crate) recs: Vec<ChunkRec>,
    free: Vec<u32>,
    map: FxHashMap<(i32, i32), u32>,
    memo: Memo,
}

#[inline]
pub(crate) fn chunk_slot_of(cell: u32) -> usize {
    (cell >> 8) as usize
}

#[inline]
pub(crate) fn col_of(cell: u32) -> usize {
    (cell & 255) as usize
}

impl Chunks {
    /// The slot of chunk `(cx, cz)` by the table: a hash, for the events (a chunk made or freed, a free position).
    pub(crate) fn find(&self, cx: i32, cz: i32) -> u32 {
        self.map.get(&(cx, cz)).copied().unwrap_or(NO_SLOT)
    }
    /// `find` through the memo: no hash for the chunks that were asked lately.
    #[inline]
    pub(crate) fn slot_at(&self, cx: i32, cz: i32) -> u32 {
        let h = ((cx as u32).wrapping_mul(0x9E37_79B1) ^ (cz as u32).wrapping_mul(0x85EB_CA6B)) >> 24;
        let s = self.memo.0[h as usize].load(Relaxed);
        if s != NO_SLOT && self.recs.get(s as usize).is_some_and(|r| r.live && r.cx == cx && r.cz == cz) {
            return s;
        }
        let s = self.find(cx, cz);
        if s != NO_SLOT {
            self.memo.0[h as usize].store(s, Relaxed);
        }
        s
    }
    /// The slot of chunk `(cx, cz)` reached from `home` by the links of the neighbours (two levels and a little more), the memo or the
    /// table past that or where a link is missing.
    #[inline]
    pub(crate) fn rel(&self, home: u32, cx: i32, cz: i32) -> u32 {
        if home == NO_SLOT {
            return self.slot_at(cx, cz);
        }
        let r = &self.recs[home as usize];
        let (mut dx, mut dz) = (cx - r.cx, cz - r.cz);
        if dx.abs() > 3 || dz.abs() > 3 {
            return self.slot_at(cx, cz);
        }
        let mut s = home;
        while dx != 0 || dz != 0 {
            let (sx, sz) = (dx.signum(), dz.signum());
            s = self.recs[s as usize].nbr[((sz + 1) * 3 + sx + 1) as usize];
            if s == NO_SLOT {
                return self.slot_at(cx, cz);
            }
            (dx, dz) = (dx - sx, dz - sz);
        }
        s
    }
    /// Makes the record of chunk `(cx, cz)` (the one it has when it is loaded already) and links its neighbours.
    pub(crate) fn create(&mut self, cx: i32, cz: i32) -> u32 {
        let known = self.find(cx, cz);
        if known != NO_SLOT {
            return known;
        }
        let slot = self.free.pop().unwrap_or_else(|| {
            self.recs.push(ChunkRec::empty());
            self.recs.len() as u32 - 1
        });
        debug_assert!(slot < 0x00FF_FFFF, "chunk slots are 24 bits");
        let mut nbr = [NO_SLOT; 9];
        nbr[4] = slot;
        for d in (0..9).filter(|&d| d != 4) {
            let n = self.find(cx + d % 3 - 1, cz + d / 3 - 1);
            nbr[d as usize] = n;
            if n != NO_SLOT {
                self.recs[n as usize].nbr[8 - d as usize] = slot;
            }
        }
        self.recs[slot as usize] = ChunkRec { cx, cz, live: true, nbr, ..ChunkRec::empty() };
        self.map.insert((cx, cz), slot);
        slot
    }
    /// Frees the record of `slot`: its neighbours forget it, the slot is reused. The entities still listed in it are returned.
    pub(crate) fn free(&mut self, slot: u32) -> Vec<Entity> {
        let r = &mut self.recs[slot as usize];
        let mut left = Vec::new();
        if let Some(c) = r.cols.take() {
            left.extend(c.cells.iter().flatten().copied());
        }
        let (cx, cz, nbr) = (r.cx, r.cz, r.nbr);
        *r = ChunkRec::empty();
        for d in (0..9).filter(|&d| d != 4) {
            if nbr[d] != NO_SLOT {
                self.recs[nbr[d] as usize].nbr[8 - d] = NO_SLOT;
            }
        }
        self.map.remove(&(cx, cz));
        self.free.push(slot);
        left
    }
    /// The loaded chunks, as slots.
    pub(crate) fn live(&self) -> impl Iterator<Item = u32> + '_ {
        self.recs.iter().enumerate().filter(|(_, r)| r.live).map(|(i, _)| i as u32)
    }
    /// The entities listed in column `cell`.
    #[inline]
    pub(crate) fn list(&self, cell: u32) -> &[Entity] {
        self.recs[chunk_slot_of(cell)].cols.as_ref().map_or(&[], |c| &c.cells[col_of(cell)])
    }
    pub(crate) fn push(&mut self, cell: u32, e: Entity) {
        let r = &mut self.recs[chunk_slot_of(cell)];
        let c = r.cols.get_or_insert_with(|| Box::new(Cols { cells: std::array::from_fn(|_| Vec::new()), slots: std::array::from_fn(|_| Vec::new()), stamp: [0; 256] }));
        let col = col_of(cell);
        c.cells[col].push(e);
        c.slots[col].push([f32::NEG_INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::INFINITY]);
        r.rows[col >> 4] |= 1 << (col & 15);
        r.len += 1;
    }
    pub(crate) fn take(&mut self, cell: u32, e: Entity) {
        let r = &mut self.recs[chunk_slot_of(cell)];
        debug_assert!(r.live, "a cell of a chunk that is freed");
        let Some(c) = r.cols.as_mut() else { return };
        let col = col_of(cell);
        if let Some(i) = c.cells[col].iter().position(|&o| o == e) {
            c.cells[col].swap_remove(i);
            c.slots[col].swap_remove(i);
            r.len -= 1;
            if c.cells[col].is_empty() {
                r.rows[col >> 4] &= !(1 << (col & 15));
            }
        }
    }
}
