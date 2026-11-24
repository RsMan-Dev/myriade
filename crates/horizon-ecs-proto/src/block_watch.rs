//! The places of the blocks some goal looks for (a repellent, say), kept as a set by events: a block that changed
//! (`ChangedBlocks`) is read again, the set takes it or drops it; nothing scans the area of a creature each tick. Each chunk is
//! indexed as it loads (`scan_chunk`), and dropped as it unloads (`forget_chunk`). `appeared` lists the names whose set
//! was empty and is not any more: the goals parked for lack of such a block wake on it.
use crate::cow::Level;
use crate::hostile::ChangedBlocks;
use bevy_ecs::prelude::*;
use std::collections::{HashMap, HashSet};

#[derive(Resource, Default)]
pub struct BlockWatch {
    sets: HashMap<&'static str, HashSet<[i32; 3]>>,
    /// The names that got their first block since the last run of the system.
    pub appeared: Vec<&'static str>,
    /// The names watched since the last `take_fresh`.
    fresh: Vec<&'static str>,
    /// The entities that wait on a cell, and the ones whose cell changed in the last run (forgotten once listed).
    cells: HashMap<[i32; 3], Vec<Entity>>,
    pub woken: Vec<Entity>,
}
impl BlockWatch {
    /// `e` waits on the block at `cell`: it is listed in `woken` when that block changes.
    pub fn watch_cell(&mut self, cell: [i32; 3], e: Entity) {
        let list = self.cells.entry(cell).or_default();
        if !list.contains(&e) {
            list.push(e);
        }
    }
    /// Starts to follow `names`: an empty set for each new one, listed in `fresh` (its blocks come from the loaded chunks, see `scan_chunk_names`).
    pub fn watch(&mut self, names: &[&'static str]) {
        for &n in names {
            if !self.sets.contains_key(n) {
                self.sets.insert(n, HashSet::new());
                self.fresh.push(n);
            }
        }
    }
    /// The names watched since the last call (the host reads the loaded chunks for them).
    pub fn take_fresh(&mut self) -> Vec<&'static str> {
        std::mem::take(&mut self.fresh)
    }
    /// The blocks of the chunk (`cx`, `cz`) enter the set of each of `names` they have (what a server reads as the chunk loads).
    pub fn scan_chunk_names(&mut self, mw: &myriade_engine_lib::mobs::MobWorld<'_>, cx: i32, cz: i32, names: &[&'static str]) {
        for x in (cx << 4)..(cx << 4) + 16 {
            for z in (cz << 4)..(cz << 4) + 16 {
                for y in -2..32 {
                    let b = mw.block_name(x, y, z);
                    for &n in names {
                        if b == n
                            && let Some(set) = self.sets.get_mut(n)
                        {
                            set.insert([x, y, z]);
                        }
                    }
                }
            }
        }
    }
    /// `scan_chunk_names` for every name already watched.
    pub fn scan_chunk(&mut self, mw: &myriade_engine_lib::mobs::MobWorld<'_>, cx: i32, cz: i32) {
        let names: Vec<&'static str> = self.sets.keys().copied().collect();
        self.scan_chunk_names(mw, cx, cz, &names);
    }
    /// The blocks of the chunk (`cx`, `cz`) leave every set when it unloads.
    pub fn forget_chunk(&mut self, cx: i32, cz: i32) {
        for set in self.sets.values_mut() {
            set.retain(|p| (p[0] >> 4, p[2] >> 4) != (cx, cz));
        }
    }
    /// Some block of one of the names exists.
    pub fn any(&self, names: &[&str]) -> bool {
        names.iter().any(|n| self.sets.get(n).is_some_and(|s| !s.is_empty()))
    }
    /// The blocks of the names within `h` blocks (and `v` up and down) of `at`.
    pub fn near(&self, names: &[&str], at: [f64; 3], h: f64, v: i32, mut f: impl FnMut([i32; 3])) {
        let (cx, cy, cz) = (at[0].floor() as i32, at[1].floor() as i32, at[2].floor() as i32);
        let r = h.floor() as i32;
        for n in names {
            for p in self.sets.get(n).into_iter().flatten() {
                if (p[0] - cx).abs() <= r && (p[2] - cz).abs() <= r && (p[1] - cy).abs() <= v {
                    f(*p);
                }
            }
        }
    }
}
pub fn insert_resources(w: &mut World) {
    w.insert_resource(BlockWatch::default());
}
pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(block_watch.after(crate::cow::block_changes).before(crate::targeting::sight_eval));
}
pub(crate) fn block_watch(level: Res<Level>, changed: Res<ChangedBlocks>, mut bw: ResMut<BlockWatch>) {
    bw.appeared.clear();
    bw.woken.clear();
    if changed.0.is_empty() {
        return;
    }
    let BlockWatch { sets, appeared, cells, woken, .. } = &mut *bw;
    for &(x, y, z) in &changed.0 {
        if let Some(list) = cells.remove(&[x, y, z]) {
            woken.extend(list);
        }
    }
    if sets.is_empty() {
        return;
    }
    let mw = level.mw.0;
    for &(x, y, z) in &changed.0 {
        let name = mw.block_name(x, y, z);
        for (k, set) in sets.iter_mut() {
            if *k == name {
                if set.is_empty() {
                    appeared.push(*k);
                }
                set.insert([x, y, z]);
            } else {
                set.remove(&[x, y, z]);
            }
        }
    }
}
