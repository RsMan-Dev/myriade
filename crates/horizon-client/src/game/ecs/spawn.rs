//! Spawns of the ECS level, as the host makes them (command, mod, spawner, trial, packs at generation): the entity is placed in
//! the level, its network id linked at once, and counted by the index. The natural spawns are the level's own (`spawning::step`).
use crate::game::Game;
use bevy_ecs::entity::Entity;
use horizon_core::nbt::Compound;
use horizon_ecs_proto::bridge::host::Select;

impl Game<'_> {
    /// Spawns `kind` at `at` in the ECS level and links its network id; None when the level is inert or refuses the kind.
    pub(in crate::game) fn ecs_spawn(&mut self, kind: &str, at: [f64; 3], nbt: Option<&Compound>) -> Option<u32> {
        let e = self.ecs_with(|ecs, mw| ecs.spawn(mw, kind, at, nbt))?;
        Some(self.ecs_link(e))
    }
    /// `ecs_spawn` for the natural spawning pass: `entity_spawn` is told `natural`.
    pub(in crate::game) fn ecs_spawn_natural(&mut self, kind: &str, at: [f64; 3]) -> Option<u32> {
        let e = self.ecs_with(|ecs, mw| ecs.spawn_natural(mw, kind, at))?;
        Some(self.ecs_link(e))
    }
    /// Loads an entity from its saved data into the ECS level and links its network id.
    pub(in crate::game) fn ecs_load(&mut self, c: &Compound) -> Option<u32> {
        let e = self.ecs_with(|ecs, mw| ecs.load_entity(mw, c))?;
        Some(self.ecs_link(e))
    }
    /// Test fixture: loads an entity of `kind` at `pos` under network id `id` (the level is opened when there is none); its id.
    #[cfg(test)]
    pub(in crate::game) fn load_other_as(&mut self, id: u32, kind: &str, pos: [f64; 3]) -> u32 {
        use crate::ecs_bridge::EcsLevel;
        use horizon_core::nbt::Tag;
        use horizon_ecs_proto::bridge::host::HostSetup;
        if !self.ecs.is_active() {
            let level = {
                self.test_ecs_level()
            };
            self.ecs = level;
        }
        let mut c = Compound::new();
        c.insert("id".into(), Tag::String(kind.into()));
        c.insert("Pos".into(), Tag::List(pos.map(|v| Tag::Double(v.to_bits())).to_vec()));
        self.next_entity = id - 1;
        self.ecs_load(&c).expect("the ECS level loads the entity")
    }
    /// The network id of `e`, linked from the game's counter when it has none.
    pub(in crate::game) fn ecs_link(&mut self, e: Entity) -> u32 {
        let next = &mut self.next_entity;
        self.ecs.id_for(e, &mut || {
            *next += 1;
            *next
        })
    }
    /// The entities of `kind` within `r` of `near`, counted by the index; not for a per-tick cap (P keeps those by events).
    pub(in crate::game) fn ecs_count(&self, kind: &str, near: [f64; 3], r: f64) -> usize {
        let q = Select { kind: Some(kind.to_owned()), tag: None, near: Some((near, r)), limit: usize::MAX };
        self.ecs.select(&q).len()
    }
}

#[cfg(test)]
mod tests {
    use crate::ecs_bridge::EcsLevel;
    use crate::game::native_entity_refs::tests::with_game;
    use horizon_ecs_proto::bridge::host::HostSetup;

    #[test]
    fn a_command_spawn_is_an_id_the_bridge_reads_and_the_count_agrees() {
        with_game(|game| {
            let level = {
                game.test_ecs_level()
            };
            game.ecs = level;
            let at = [0.5, 64.0, 0.5];
            let first = game.ecs_spawn("minecraft:cow", at, None).unwrap();
            let second = game.ecs_spawn("minecraft:cow", [1.5, 64.0, 0.5], None).unwrap();
            assert!(game.ecs.exists(first) && game.ecs.exists(second));
            assert_eq!(game.ecs.kind(first), Some("minecraft:cow"));
            assert_eq!(game.ecs_count("minecraft:cow", at, 8.0), 2);
            assert_eq!(game.ecs_count("minecraft:cow", [100.0, 64.0, 100.0], 8.0), 0);
        });
    }
}
