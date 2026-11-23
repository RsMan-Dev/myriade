//! The world update (`update_world`): what arrived from the server, the chunks that
//! streamed in, the columns that unload (their entities and block entities saved out first).
use super::*;

impl Game<'_> {
    pub(in crate::game) fn update_world(&mut self) {
        self.ask_edited_light_emission();
        if let Some(p) = &self.player {
            self.streaming.center = (
                (p.position.x.floor() as i32) >> 4,
                (p.position.z.floor() as i32) >> 4,
            );
        }
        myriade_engine::spans::time("tick_pregen", || self.tick_pregen());
        let areas = self.streaming.extra.len();
        let waiting = std::mem::take(&mut self.spawn_wait);
        self.streaming
            .extra
            .extend(waiting.into_iter().map(|chunk| (chunk, 0)));
        let plan = myriade_engine::spans::time("streaming_plan", || {
            let mut step = self.streaming.begin_plan_update(&mut self.world);
            loop {
                match step {
                    myriade_engine::streaming::PlanStep::Ready(plan) => break plan,
                    myriade_engine::streaming::PlanStep::Save { chunk, resume } => {
                        self.native_late_full_chunk_snapshot(&chunk);
                        self.streaming.save_finished(chunk);
                        step = self.streaming.resume_plan_update(&mut self.world, resume);
                    }
                }
            }
        });
        let (commit, protos) = self.streaming.prepare_commit(&mut self.world, plan);
        self.native_proto_snapshots(&protos);
        self.streaming.submit_protos(protos);
        let unloaded = myriade_engine::spans::time("streaming", || {
            self.streaming.complete_commit(&mut self.world, commit)
        });
        self.streaming.extra.truncate(areas);
        let dirty: Vec<_> = unloaded
            .iter()
            .filter(|c| self.world.modified.remove(&(c.x, c.z)))
            .cloned()
            .collect();
        let modified = myriade_engine::spans::time("with_entities", || self.with_entities(&dirty));
        self.native_full_chunk_snapshots(&modified);
        if !modified.is_empty()
            && let Err(e) = myriade_engine::spans::time("save_chunks", || {
                self.save
                    .save_later(&modified, self.pack.assets.registry, world_biomes!(self))
            })
        {
            log::warn!("Horizon: failed to save unloaded chunks: {e}");
        }
        myriade_engine::spans::time("spawn_fresh_chunks", || self.spawn_fresh_chunks());
        self.ask_loaded_light_emission();
        // Vanilla saves the chunk before its unload hook and drops its block entities afterwards.
        myriade_engine::spans::time("mods_chunks_changed", || {
            self.native_chunks_loaded();
            self.native_chunks_unloaded(&unloaded);
        });
        for chunk in &unloaded {
            self.world.modified.unloaded(&(chunk.x, chunk.z));
        }
        for chunk in &unloaded {
            self.world.modified.unloaded(&(chunk.x, chunk.z));
        }
        myriade_engine::spans::time("unload_entities", || self.unload_entities(&unloaded));
        if !unloaded.is_empty() {
            let _span = myriade_engine::spans::guard("unload_block_entities");
            let gone: myriade_engine::fast_hash::FxHashSet<(i32, i32)> =
                unloaded.iter().map(|c| (c.x, c.z)).collect();
            let gone_chunks: Vec<(i32, i32)> = gone.iter().copied().collect();
            self.world.block_entities.unload_chunks(&gone_chunks);
            self.world
                .block_names
                .retain(|p, _| !gone.contains(&(p.0 >> 4, p.2 >> 4)));
        }
        // LevelChunk.registerTickContainerInLevel: a chunk that became
        // resident has its tick containers before a command can schedule into
        // it, even in a frozen level that runs no world tick. The unloaded
        // chunks above were saved with their ticks first.
        {
            myriade_engine::spans::time("sync_tick_chunks", || self.sync_tick_chunks());
        }
    }
}
