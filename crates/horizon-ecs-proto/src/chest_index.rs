//! The chests of the world by chunk, kept by events: a block that changed (`ChangedBlocks`) is read again, the chest is listed or
//! dropped; a search for a chest (the copper golem's) reads the chunks of its box and nothing else, with no walk of the block entities.
//! The world of the bench is read once when the first golem looks (a server would list each chunk as it loads); the resource does not
//! exist in a world that has no such golem.
use crate::cow::Level;
use crate::hostile::ChangedBlocks;
use bevy_ecs::prelude::*;
use horizon_client_lib::containers::BlockEntity;
use horizon_client_lib::mobs::MobWorld;
use std::collections::HashMap;

#[derive(Resource, Default)]
pub struct ChestIndex {
    chunks: HashMap<(i32, i32), Vec<[i32; 3]>>,
    built: bool,
}
impl ChestIndex {
    /// Lists the chests of the world once.
    pub fn ensure(&mut self, mw: &MobWorld) {
        if self.built {
            return;
        }
        self.built = true;
        for (k, be) in &mw.world.block_entities {
            if matches!(be, BlockEntity::Chest(_)) {
                self.chunks.entry((k.0 >> 4, k.2 >> 4)).or_default().push([k.0, k.1, k.2]);
            }
        }
    }
    /// The chests of chunk `(cx, cz)`, in no order.
    pub fn chunk(&self, cx: i32, cz: i32) -> &[[i32; 3]] {
        self.chunks.get(&(cx, cz)).map_or(&[], Vec::as_slice)
    }
    fn update(&mut self, mw: &MobWorld, p: [i32; 3]) {
        let list = self.chunks.entry((p[0] >> 4, p[2] >> 4)).or_default();
        list.retain(|q| *q != p);
        if matches!(mw.world.block_entities.get(&(p[0], p[1], p[2])), Some(BlockEntity::Chest(_))) {
            list.push(p);
        }
    }
}

pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(chest_changes.after(crate::cow::block_changes).before(crate::targeting::sight_eval));
}

fn chest_changes(level: Res<Level>, changed: Res<ChangedBlocks>, idx: Option<ResMut<ChestIndex>>) {
    let Some(mut idx) = idx else { return };
    if !idx.built {
        return;
    }
    for &(x, y, z) in &changed.0 {
        idx.update(level.mw.0, [x, y, z]);
    }
}
