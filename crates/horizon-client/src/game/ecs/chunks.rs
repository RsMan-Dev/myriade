//! The ECS world's entities by chunk: their NBT as a chunk saves it, and the chunk's load and unload as events of the world.
//! Nothing here scans the entities: a chunk is asked by its key, and the world answers from its index.
use crate::game::Game;
use crate::mobs::Who;
use horizon_core::nbt::{Compound, Tag};
use horizon_ecs_proto::nbt;

impl Game<'_> {
    /// The entities of the chunk on disk that the world could not make (their id is not declared), read again only when the world
    /// flagged the chunk for them (`has_unknown`). Empty otherwise.
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
        self.ecs.chunk_loaded(cx, cz);
        if self.ecs.save_chunk(cx, cz).is_empty() {
            for c in entities {
                self.ecs.load_entity(c);
            }
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

#[cfg(test)]
mod tests {
    use crate::ecs_bridge::EcsLevel;
    use crate::mobs::world::leaked_test_world_mut;
    use horizon_core::nbt::{JavaString, Tag};
    use horizon_ecs_proto::bridge::host::HostSetup;

    fn with_level(f: impl FnOnce(&mut EcsLevel)) {
        let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("vanilla test resources required");
        let setup = HostSetup { seed: 0, dimension: "minecraft:overworld", simulation_radius: 4 };
        let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
        f(&mut EcsLevel::new(&setup, mw, types, &tags));
    }

    #[test]
    fn a_chunk_without_entities_saves_an_empty_list() {
        with_level(|level| assert!(level.save_chunk(5, 5).is_empty()));
    }

    #[test]
    fn a_key_no_field_declares_survives_a_save_and_a_load() {
        with_level(|level| {
            let e = level.spawn("minecraft:cow", [1.5, 64.0, 1.5], None).unwrap();
            let mut saved = level.save_chunk(0, 0);
            assert_eq!(saved.len(), 1);
            let mut c = saved.remove(0);
            c.insert("ModKey".into(), Tag::Int(7));
            level.despawn(e);
            assert!(level.load_entity(&c).is_some());
            let back = level.save_chunk(0, 0);
            assert_eq!(back[0].get(&JavaString::from("ModKey")), Some(&Tag::Int(7)));
        });
    }

    #[test]
    fn a_sleeping_entity_is_saved() {
        with_level(|level| {
            let e = level.spawn("minecraft:cow", [1.5, 64.0, 1.5], None).unwrap();
            let now = 0;
            horizon_ecs_proto::zone::sleep(level.world_mut().unwrap(), e, now);
            assert_eq!(level.save_chunk(0, 0).len(), 1);
        });
    }
}
