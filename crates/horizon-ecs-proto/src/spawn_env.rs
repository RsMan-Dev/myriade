//! What the host tells the natural spawner about the level, by event: the rules, the chunks that may take a creature, the border and the
//! world spawn. The pass reads it each tick; nothing here is scanned from the world.
use bevy_ecs::prelude::*;
use horizon_core::fast_hash::FxHashSet;

/// `SPAWN_MOBS` and `SPAWN_MONSTERS` (`ServerLevel.isSpawningMonsters`), the chunks that may take a creature, the world border
/// `[min_x, min_z, max_x, max_z]` and the world spawn.
#[derive(Resource, Default)]
pub struct SpawnEnv {
    pub spawn_mobs: bool,
    pub monsters: bool,
    /// The chunks that tick and that no ticket holds back from natural spawning.
    pub spawnable: FxHashSet<(i32, i32)>,
    pub border: Option<[f64; 4]>,
    pub world_spawn: [i32; 3],
    /// Set when a chunk or the border changed: the pass makes its candidates again, as when a player changes chunk.
    pub changed: bool,
}

impl SpawnEnv {
    /// `ServerLevel.canSpawnEntitiesInChunk`: the chunk ticks, and the border holds both its corners.
    pub fn may_spawn_in(&self, (cx, cz): (i32, i32)) -> bool {
        self.spawnable.contains(&(cx, cz)) && self.border.is_none_or(|b| inside(b, cx * 16, cz * 16) && inside(b, cx * 16 + 15, cz * 16 + 15))
    }
}

/// `WorldBorder.isWithinBounds`: from `min` to `max`, `max` excluded.
fn inside(b: [f64; 4], x: i32, z: i32) -> bool {
    let (x, z) = (f64::from(x), f64::from(z));
    x >= b[0] && x < b[2] && z >= b[1] && z < b[3]
}
