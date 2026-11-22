//! The ECS world's entities by chunk: their NBT as a chunk saves it, and the chunk's load and unload as events of the world.
//! Nothing here scans the entities: a chunk is asked by its key, and the world answers from its index.
use crate::entity_nbt;
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
            .filter(|t| !entity_nbt::kind(t).is_some_and(|k| crate::game::general::quarantine_store_kind(&k)))
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
        self.ecs_with(|ecs, mw| ecs.chunk_loaded(mw, cx, cz));
        if self.ecs.save_chunk(cx, cz).is_empty() {
            let seed = self.level.seed;
            let tops = self.ecs_with(|ecs, mw| {
                let loaded: Vec<_> = entities.iter().filter_map(|c| ecs.load_entity(mw, c)).collect();
                let Some(w) = ecs.ecs().map(|w| w.world()) else { return 0 };
                loaded
                    .into_iter()
                    .filter(|&e| {
                        w.get::<horizon_ecs_proto::end_crystal::EndCrystal>(e).is_some()
                            && w.get::<horizon_ecs_proto::cow::Pos>(e).is_some_and(|p| crate::game::dragon::crystal::spike_top(seed, p.0))
                    })
                    .count()
            });
            self.fight.alive_crystals += tops as i32;
            self.ecs_resolve_pending_leashes();
        }
    }

    /// Saves chunk (cx, cz) and then unloads it: the world keeps the entities' NBT and drops them from the level.
    pub(in crate::game) fn ecs_unload_chunk(&mut self, cx: i32, cz: i32, disk: Vec<Compound>) -> Vec<Compound> {
        let saved = self.ecs_save_chunk(cx, cz, disk);
        self.ecs_with(|ecs, mw| ecs.chunk_unloaded(mw, cx, cz));
        saved
    }

    /// A mob of the level that is not removed: in the ECS world when it runs, else in the legacy list.
    pub(in crate::game) fn mob_present(&self, id: u32) -> bool {
        if self.ecs.is_active() { self.ecs.exists(id) } else { self.remote_entities.iter().any(|m| m.id == id && !m.removed) }
    }

    /// A mob of the level, removed or not (the ECS world drops a removed entity at once).
    pub(in crate::game) fn mob_known(&self, id: u32) -> bool {
        if self.ecs.is_active() { self.ecs.exists(id) } else { self.remote_entities.iter().any(|m| m.id == id) }
    }

    /// The UUID-named entity of the ECS world that holds `uuid`, as a mob. None without an ECS world.
    pub(in crate::game) fn ecs_who_by_uuid(&self, uuid: [i32; 4]) -> Option<Who> {
        if !self.ecs.is_active() {
            return None;
        }
        self.persist.uuids.iter().find(|(id, u)| **u == uuid && self.ecs.exists(**id)).map(|(id, _)| Who::Mob(*id))
    }
}
