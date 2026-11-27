//! The ECS world's entities by chunk: their NBT as a chunk saves it, and the chunk's load and unload as events of the world.
//! Nothing here scans the entities: a chunk is asked by its key, and the world answers from its index.
use crate::game::Game;
use crate::mobs::Who;
use horizon_core::nbt::{Compound, Tag};
use horizon_ecs_proto::nbt;

impl Game<'_> {
    /// The entities of the chunk on disk that the world could not make (their id is not declared), read again only when the world
    /// flagged the chunk for them (`has_unknown`). Empty otherwise. A missing mod's record is not among them: its own store keeps it.
    pub(in crate::game) fn ecs_disk_unknown(&self, cx: i32, cz: i32) -> Vec<Compound> {
        if !self.ecs.world().is_some_and(|w| nbt::has_unknown(w, cx, cz)) {
            return Vec::new();
        }
        self.save
            .load_entities(cx, cz)
            .into_iter()
            .filter_map(|t| match t {
                Tag::Compound(c) => Some(c),
                _ => None,
            })
            .collect()
    }

    /// The NBT of the entities of chunk (cx, cz), in the Minecraft format (passengers under `Passengers`), with the entities
    /// of `disk` the world kept without an id (`keep_unknown`).
    pub(in crate::game) fn ecs_save_chunk(&mut self, cx: i32, cz: i32, disk: Vec<Compound>) -> Vec<Compound> {
        let mut saved = self.ecs.save_chunk(cx, cz);
        if let Some(w) = self.ecs.world_mut() {
            saved.extend(nbt::keep_unknown(w, cx, cz, disk.into_iter()));
        }
        saved
    }

    /// Restores chunk (cx, cz) after it loaded. The world gives back the entities it kept when the chunk unloaded in this
    /// session; the file's entities are made only when it gave none back (a restart), so none is made twice.
    pub(in crate::game) fn ecs_load_chunk(&mut self, cx: i32, cz: i32, entities: &[Compound]) {
        // Only a chunk at `FULL` or better has accessible entities (`ChunkLevel.fullStatus`); the others wait, saved with it, until
        // a ticket level change brings it there (`ecs_ticket_levels_changed`).
        if !self.tickets.levels().full(cx, cz) {
            // The world kept this chunk's entities when it unloaded in this session: the file's are the same, held once.
            if self.ecs.save_chunk(cx, cz).is_empty() {
                self.ecs.hold_chunk(cx, cz, entities.to_vec());
            }
            return;
        }
        self.ecs_with(|ecs, mw| ecs.chunk_loaded(mw, cx, cz));
        if self.ecs.save_chunk(cx, cz).is_empty() {
            // The fight's crystal count is the ECS's: a loaded crystal is counted there.
            self.ecs_with(|ecs, mw| {
                for c in entities {
                    ecs.load_entity(mw, c);
                }
            });
            self.ecs_resolve_pending_leashes();
        }
    }

    /// Saves chunk (cx, cz) and then unloads it: the world keeps the entities' NBT and drops them from the level.
    pub(in crate::game) fn ecs_unload_chunk(&mut self, cx: i32, cz: i32, disk: Vec<Compound>) -> Vec<Compound> {
        let saved = self.ecs_save_chunk(cx, cz, disk);
        self.ecs_with(|ecs, mw| ecs.chunk_unloaded(mw, cx, cz));
        self.ecs_chunk_loaded([cx, cz], false);
        saved
    }

    /// A mob of the level that is not removed: in the ECS world.
    pub(in crate::game) fn mob_present(&self, id: u32) -> bool {
        self.ecs.exists(id)
    }

    /// A mob of the level, removed or not (the ECS world drops a removed entity at once).
    pub(in crate::game) fn mob_known(&self, id: u32) -> bool {
        self.ecs.exists(id)
    }

    /// The UUID-named entity of the ECS world that holds `uuid`, as a mob. None without an ECS world.
    pub(in crate::game) fn ecs_who_by_uuid(&self, uuid: [i32; 4]) -> Option<Who> {
        if !self.ecs.is_active() {
            return None;
        }
        self.persist.uuids.iter().find(|(id, u)| **u == uuid && self.ecs.exists(**id)).map(|(id, _)| Who::Mob(*id))
    }
}
