//! Mods' reads and writes of the ECS entities of this level, by network id: a flag or a number of the entity's NBT
//! (`EcsLevel::entity_nbt`, `merge_nbt`), under the keys vanilla saves. Nothing here scans the world.
use super::super::Game;
use myriade_engine::{ecs_bridge::EcsLevel, mobs::MobWorld};
use horizon_core::nbt::{Compound, JavaString, Tag};

impl Game<'_> {
    /// Runs `f` on this level with the mob world lent for the call. The level is taken out of `self` meanwhile, since
    /// the mob world borrows `self`.
    pub(in crate::game) fn with_ecs<R>(&mut self, f: impl FnOnce(&mut EcsLevel, &MobWorld<'_>) -> R) -> R {
        let mut ecs = std::mem::replace(&mut self.ecs, EcsLevel::inert());
        let out = f(&mut ecs, &self.mob_world_base(&[]));
        self.ecs = ecs;
        out
    }
    /// Whether network id `id` names an entity of this level's ECS world.
    pub(in crate::game) fn ecs_has(&self, id: u32) -> bool {
        self.ecs.entity_of(id).is_some()
    }
    /// A flag of the ECS entity `id`, saved as a byte that is absent when unset (`Entity.saveWithoutId`).
    pub(in crate::game) fn ecs_flag(&self, id: u32, key: &str) -> Option<bool> {
        let nbt = self.ecs.entity_nbt(self.ecs.entity_of(id)?)?;
        Some(matches!(nbt.get(&JavaString::from(key)), Some(Tag::Byte(v)) if *v != 0))
    }
    /// A short or int number of the ECS entity `id`, 0 when unset.
    pub(in crate::game) fn ecs_number(&self, id: u32, key: &str) -> Option<i32> {
        let nbt = self.ecs.entity_nbt(self.ecs.entity_of(id)?)?;
        Some(match nbt.get(&JavaString::from(key)) {
            Some(Tag::Short(v)) => i32::from(*v),
            Some(Tag::Int(v)) => *v,
            _ => 0,
        })
    }
    /// Merges `value` under `key` into the ECS entity `id`; false when it has no entity.
    pub(in crate::game) fn ecs_put(&mut self, id: u32, key: &str, value: Tag) -> bool {
        let Some(e) = self.ecs.entity_of(id) else {
            return false;
        };
        self.ecs.merge_nbt(e, &Compound::from([(JavaString::from(key), value)]))
    }
    /// Writes a flag as vanilla saves it: a byte 1 when set, a byte 0 when cleared (read the same as absent).
    pub(in crate::game) fn ecs_put_flag(&mut self, id: u32, key: &str, on: bool) -> bool {
        self.ecs_put(id, key, Tag::Byte(i8::from(on)))
    }
}

#[cfg(test)]
mod tests {
    use crate::ecs_bridge::EcsLevel;
    use crate::game::native_entity_refs::tests::with_game;
    use horizon_ecs_proto::bridge::host::HostSetup;

    #[test]
    fn a_flag_written_by_a_mod_reads_back_on_the_ecs_mob_and_clears_when_unset() {
        with_game(|game| {
            let level = {
                game.test_ecs_level()
            };
            game.ecs = level;
            let id = game.ecs_spawn("minecraft:cow", [0.5, 64.0, 0.5], None).unwrap();
            assert!(game.ecs_has(id));
            assert!(game.ecs_put_flag(id, "Silent", true));
            assert_eq!(game.ecs_flag(id, "Silent"), Some(true));
            assert!(game.ecs_put_flag(id, "Silent", false));
            assert_eq!(game.ecs_flag(id, "Silent"), Some(false));
        });
    }

    #[test]
    fn a_despawned_ecs_mob_frees_its_id_for_the_mods() {
        with_game(|game| {
            let level = {
                game.test_ecs_level()
            };
            game.ecs = level;
            let id = game.ecs_spawn("minecraft:cow", [0.5, 64.0, 0.5], None).unwrap();
            let e = game.ecs.entity_of(id).unwrap();
            game.ecs.despawn(e);
            assert!(!game.ecs_has(id));
            assert_eq!(game.ecs_flag(id, "Silent"), None);
        });
    }
}
