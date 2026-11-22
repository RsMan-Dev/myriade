//! Mob spawning and a level's natural spawning policy through a `LevelRef`
//! (docs/modding/src/how-to/world/spawning.md, "Spawning from a level"): a mob made
//! as `EntityType.spawn` makes one, the per-category limits and the flags
//! natural spawning reads (`natural_spawning::SpawnPolicy`), and the chunk
//! question it asks (`ServerLevel.canSpawnEntitiesInChunk`).
use super::{Game, creatures::SpawnSite, natural_spawning};
use mod_api::{EntityRef, LevelRef, SpawnFlags, SpawnLimits};

impl Game<'_> {
    /// `EntityType.spawn(level, pos, reason)` for a mob: `spawn_mob_spawned`'s path
    /// (creation, `finalize_spawn` and `finalizeSpawn` with `reason`, the join), not
    /// persistent. None for a level not in place, a client, a position that isn't finite,
    /// outside the spawnable bounds or in an unloaded chunk, a kind that isn't a mob, and
    /// a spawn a mod cancelled or kept out.
    pub(super) fn level_spawn_mob(
        &mut self,
        level: LevelRef,
        kind: &str,
        pos: [f64; 3],
        reason: mod_api::SpawnReason,
    ) -> Option<EntityRef> {
        if self.entity_ids_exhausted() {
            return None;
        }
        if !self.level_in_place(level) || !pos.iter().all(|v| v.is_finite()) {
            return None;
        }
        // BlockPos.containing. `as` saturates, and a value past `i32` is out of bounds.
        let block = pos.map(|v| v.floor() as i32);
        if self.level_in_spawnable_bounds(level, block) != Some(true)
            || !self.world.loaded(block[0], block[2])
        {
            return None;
        }
        let id = self.spawn_mob_spawned(kind, pos, SpawnSite::Finalizing(reason))?;
        Some(EntityRef::new(level.epoch(), id))
    }
    /// The level's natural spawning limits. None for a stale handle or on a client.
    pub(super) fn level_spawn_limits(&self, level: LevelRef) -> Option<SpawnLimits> {
        self.native_level_world(level)?;
        Some(self.spawn_policy(level.dimension()).limits)
    }
    /// The level's natural spawning flags. None for a stale handle or on a client.
    pub(super) fn level_spawn_flags(&self, level: LevelRef) -> Option<SpawnFlags> {
        self.native_level_world(level)?;
        Some(self.spawn_policy(level.dimension()).flags)
    }
    /// Sets the level's natural spawning limits for the rest of the open. False for a
    /// stale handle or on a client.
    pub(super) fn level_set_spawn_limits(&mut self, level: LevelRef, limits: SpawnLimits) -> bool {
        self.native_set_spawn_policy(level, |policy| policy.limits = limits)
    }
    /// Sets the level's natural spawning flags for the rest of the open. False as for
    /// `level_set_spawn_limits`.
    pub(super) fn level_set_spawn_flags(&mut self, level: LevelRef, flags: SpawnFlags) -> bool {
        self.native_set_spawn_policy(level, |policy| policy.flags = flags)
    }
    fn native_set_spawn_policy(
        &mut self,
        level: LevelRef,
        set: impl FnOnce(&mut natural_spawning::SpawnPolicy),
    ) -> bool {
        if self.native_level_world(level).is_none() {
            return false;
        }
        set(self.spawn_policies.entry(level.dimension()).or_default());
        true
    }
    /// `ServerLevel.canSpawnEntitiesInChunk` as natural spawning answers it. None for a
    /// stale handle or on a client.
    pub(super) fn level_can_spawn_entities_in_chunk(
        &self,
        level: LevelRef,
        pos: [i32; 2],
    ) -> Option<bool> {
        let world = self.native_level_world(level)?;
        Some(natural_spawning::can_spawn_entities_in_chunk(
            world, pos[0], pos[1],
        ))
    }
}
