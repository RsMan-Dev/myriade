//! The places of the blocks some goal looks for (a repellent, say), kept as a set by events: a block that changed
//! (`ChangedBlocks`) is read again, the set takes it or drops it; nothing scans the area of a creature each tick. The slab is
//! indexed once when a name is first watched (a server would index each chunk as it loads). `appeared` lists the names whose set
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
}
impl BlockWatch {
    /// Starts to follow `names` (the slab is read once for each new one).
    pub fn watch(&mut self, level: &Level, names: &[&'static str]) {
        for &n in names {
            if self.sets.contains_key(n) {
                continue;
            }
            let mw = level.mw.0;
            let mut set = HashSet::new();
            for x in -level.half..=level.half {
                for z in -level.half..=level.half {
                    for y in -2..32 {
                        if mw.block_name(x, y, z) == n {
                            set.insert([x, y, z]);
                        }
                    }
                }
            }
            self.sets.insert(n, set);
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
fn block_watch(level: Res<Level>, changed: Res<ChangedBlocks>, mut bw: ResMut<BlockWatch>) {
    bw.appeared.clear();
    if changed.0.is_empty() || bw.sets.is_empty() {
        return;
    }
    let mw = level.mw.0;
    let BlockWatch { sets, appeared } = &mut *bw;
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
