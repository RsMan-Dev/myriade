//! The native mob files' side of the ECS: the mobs a mod reads or writes by network id, when the ECS runs the world (the server
//! roles). Reads go to the view (`ecs.exists`, `ecs.alive`, the `access` reads); writes are requests the level applies.
use crate::ecs_bridge::EcsLevel;
use crate::game::Game;
use crate::mobs::MobWorld;
use bevy_ecs::entity::Entity;
use horizon_ecs_proto::bridge::host::Select;

impl Game<'_> {
    /// Lends the ECS level and the mob world to `f` together: the level leaves the game for the call, since the view of the game
    /// borrows it otherwise (as `tick_ecs` does for the step).
    pub(in crate::game) fn ecs_with_mw<R>(&mut self, f: impl FnOnce(&mut EcsLevel, &MobWorld<'_>) -> R) -> R {
        let mut ecs = std::mem::replace(&mut self.ecs, EcsLevel::inert());
        let r = f(&mut ecs, &self.mob_world_base(&[]));
        self.ecs = ecs;
        r
    }

    /// The entity of the mob `id` when it lives in the ECS: asleep, removed or dead mobs are absent to a mod.
    pub(in crate::game) fn ecs_mod_live(&self, id: u32) -> Option<Entity> {
        let e = self.ecs.entity_of(id)?;
        (self.ecs.exists(id) && self.ecs.alive(id)).then_some(e)
    }

    /// The network ids of every mob the ECS holds, linking the ones that have none (a mod's list names them). Reads the whole
    /// index once per call, as a mod's list does; no per-tick scan.
    pub(in crate::game) fn ecs_mod_mobs(&mut self) -> Vec<u32> {
        let q = Select { kind: None, tag: None, near: None, limit: usize::MAX };
        let ids: Vec<u32> = self.ecs.select(&q).into_iter().map(|e| self.ecs_link(e)).collect();
        ids.into_iter().filter(|&id| self.ecs.is_mob(id)).collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::ecs_bridge::EcsLevel;
    use crate::game::native_entity_refs::tests::with_game;
    use horizon_ecs_proto::bridge::host::HostSetup;

    #[test]
    fn a_mod_reads_a_position_through_the_view() {
        with_game(|game| {
            let level = {
                game.test_ecs_level()
            };
            game.ecs = level;
            let at = [0.5, 64.0, 0.5];
            let id = game.ecs_spawn("minecraft:cow", at, None).unwrap();
            assert!(game.ecs_mod_live(id).is_some());
            assert_eq!(game.ecs.position(id), Some(at));
        });
    }

    #[test]
    fn a_mod_writes_a_position_through_teleport() {
        with_game(|game| {
            let level = {
                game.test_ecs_level()
            };
            game.ecs = level;
            let id = game.ecs_spawn("minecraft:cow", [0.5, 64.0, 0.5], None).unwrap();
            let to = [4.5, 64.0, 4.5];
            game.ecs_with_mw(|ecs, mw| {
                let e = ecs.entity_of(id).unwrap();
                ecs.teleport(mw, e, to);
            });
            assert_eq!(game.ecs.position(id), Some(to));
        });
    }

    #[test]
    fn an_invalid_reference_is_absent_not_a_panic() {
        with_game(|game| {
            let level = {
                game.test_ecs_level()
            };
            game.ecs = level;
            assert!(game.ecs_mod_live(4_000_000).is_none());
            assert_eq!(game.ecs.position(4_000_000), None);
        });
    }
}
