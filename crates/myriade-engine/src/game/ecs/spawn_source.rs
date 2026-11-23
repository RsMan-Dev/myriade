//! The spawner's inputs from the game: the structure starts of a chunk, its creature chunks, the border and the world spawn. Each is
//! pushed at the event that changes it (a chunk loads or unloads, the border or the spawn moves); nothing here runs per tick.
use super::super::Game;
use myriade_engine::ecs_bridge::EcsLevel;
use myriade_engine::ecs_bridge::level::ChunkStart;
use myriade_engine::ticket_levels::LevelChange;

impl Game<'_> {
    /// The structure starts that the references of chunk `chunk` name, owned: their id, their box and their pieces' boxes as placed.
    fn ecs_chunk_starts(&self, chunk: [i32; 2]) -> Vec<ChunkStart> {
        let (Some(loaded), Some(locator)) = (self.world.chunks.get(&(chunk[0], chunk[1])), self.end.locator()) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (id, (sx, sz)) in &loaded.structure_refs {
            for start in locator.starts_at(*sx, *sz).iter().filter(|s| *s.name == **id) {
                out.push(ChunkStart {
                    id: id.to_string(),
                    bounds: start.bounding_box,
                    pieces: start.pieces.iter().map(|p| p.placed_bounding_box()).collect(),
                });
            }
        }
        out
    }

    /// A chunk loads (`on`) or unloads: its structure starts, and whether it takes creatures (`block_ticking_at`).
    pub(in crate::game) fn ecs_chunk_loaded(&mut self, chunk: [i32; 2], on: bool) {
        let starts = if on { self.ecs_chunk_starts(chunk) } else { Vec::new() };
        self.ecs.set_chunk_structures(chunk, starts);
        // Its entities load in the same pass (`spawn_loaded_entities`), so only the ticket levels decide.
        self.ecs.set_chunk_spawnable(chunk[0], chunk[1], on && self.tickets.levels().block_ticking_at(chunk[0] * 16, chunk[1] * 16));
    }

    /// The ticket levels changed by `changes` (one update's): a loaded chunk among them takes or leaves creatures (`block_ticking_at`).
    pub(in crate::game) fn ecs_ticket_levels_changed(&mut self, changes: &[LevelChange]) {
        for change in changes {
            let (cx, cz) = change.chunk;
            if self.world.chunks.contains_key(&(cx, cz)) {
                self.ecs.set_chunk_spawnable(cx, cz, self.block_ticking_at(cx * 16, cz * 16));
            }
        }
    }

    /// The border's bounds or centre changed.
    pub(in crate::game) fn ecs_border_changed(&mut self) {
        let bounds = self.border.geometry().bounds(0.0);
        self.ecs.set_world_border(Some(bounds));
    }

    /// The world spawn moved to `at`.
    pub(in crate::game) fn ecs_world_spawn_changed(&mut self, at: [i32; 3]) {
        self.ecs.set_world_spawn(at);
    }

    /// The state of a new ECS level: the loaded chunks, the border and the world spawn, as the events would have pushed them.
    pub(in crate::game) fn ecs_initial_pose(&self, ecs: &mut EcsLevel) {
        for &(cx, cz) in self.world.chunks.keys() {
            ecs.set_chunk_structures([cx, cz], self.ecs_chunk_starts([cx, cz]));
            ecs.set_chunk_spawnable(cx, cz, self.block_ticking_at(cx * 16, cz * 16));
        }
        ecs.set_world_border(Some(self.border.geometry().bounds(0.0)));
        if let Some(at) = self.level.world_spawn {
            ecs.set_world_spawn(at);
        }
    }
}
