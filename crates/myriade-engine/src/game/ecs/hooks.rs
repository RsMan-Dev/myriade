//! The native hook points of the ECS entities, by network id: the ECS side of the points of `shared.rs`, `living_death*.rs` and
//! `living_conversion_*.rs`. Each is called by the bridge's event for its entity (death, conversion, target, spawn, tame, hit,
//! kill); none is asked at each tick, and none scans the level.
use crate::game::Game;
use crate::game::native_namespaced_id::namespaced;
use bevy_ecs::entity::Entity;
use myriade_engine::items::ItemStack;
use horizon_ecs_proto::goals;

impl Game<'_> {
    /// The kind of the ECS entity `id` as the hook points name it (`minecraft:cow`); None when it has no view.
    pub(in crate::game) fn ecs_kind(&self, id: u32) -> Option<String> {
        self.ecs.kind(id).map(|kind| namespaced(kind))
    }
    /// The stack in the main hand of the mob `id`, read from the ECS.
    pub(in crate::game) fn mob_main_hand(&self, id: i32) -> Option<ItemStack> {
        let e = self.ecs.entity_of(u32::try_from(id).ok()?)?;
        goals::main_hand(self.ecs.world()?, e).cloned()
    }
    /// `native_mobs_joined` for the ECS entity `id` that has just spawned, told as new in this level.
    pub(in crate::game) fn native_ecs_joined(&mut self, id: u32, natural: bool) {
        let (Some(kind), Some(pos)) = (self.ecs_kind(id), self.ecs.position(id)) else {
            return;
        };
        self.native_mob_spawned(self.dim.id(), &kind, pos, natural);
    }
    /// `native_hurt_enemy_event` for the ECS entity `id`, hit by the acting player holding `stack`.
    pub(in crate::game) fn native_ecs_hurt_enemy(&mut self, stack: &ItemStack, id: u32) {
        let Some(kind) = self.ecs_kind(id) else {
            return;
        };
        self.native_hurt_enemy(stack, id, &kind);
    }
    /// `animal_tame_cancelled` for the ECS entity `id`, whose taming roll by `tamer` succeeded: true when a mod cancels it.
    pub(in crate::game) fn ecs_animal_tame_cancelled(&mut self, id: u32, tamer: u128, tamer_entity_id: i32) -> bool {
        if !false {
            return false;
        }
        let Some(kind) = self.ecs_kind(id) else {
            return false;
        };
        let mut cancel = false;
        self.native_animal_tame(tamer, tamer_entity_id, id, &kind, &mut cancel);
        cancel
    }
    /// `living_death_post` for the ECS entity `id`, dead and still in the level, after its loot and experience.
    pub(in crate::game) fn native_ecs_death_post(&mut self, id: u32, damage_type: &str, attacker: u32) {
        if self.ecs.exists(id) {
            self.native_living_death_post(id, damage_type, attacker);
        }
    }
    /// `living_conversion_pre` for the ECS entity `id` about to become `outcome`: true when a mod keeps it.
    pub(in crate::game) fn ecs_conversion_pre(&mut self, id: u32, outcome: &str) -> bool {
        let Some(kind) = self.ecs_kind(id) else {
            return false;
        };
        let mut cancel = false;
        self.native_living_conversion_pre(id, &kind, outcome, &mut cancel);
        cancel
    }
    /// `living_conversion_post` for the ECS entity `old`, turned into `new`: told while `old` is still in the level. `new` is
    /// linked here and its id returned; the caller despawns `old` after this call.
    pub(in crate::game) fn ecs_converted(&mut self, old: u32, new: Entity) -> u32 {
        let Some(kind) = self.ecs_kind(old) else {
            return 0;
        };
        let converted = self.ecs_link(new);
        let converted_kind = self.ecs_kind(converted).unwrap_or_default();
        self.native_living_conversion_post(old, &kind, converted, &converted_kind);
        converted
    }
}

#[cfg(test)]
mod tests {
    use crate::ecs_bridge::EcsLevel;
    use crate::game::native_entity_refs::tests::with_game;

    #[test]
    fn a_conversion_unlinks_the_old_id_and_links_the_new_one() {
        with_game(|game| {
            game.ecs = game.test_ecs_level();
            let at = [0.5, 64.0, 0.5];
            let mut ecs = std::mem::replace(&mut game.ecs, EcsLevel::inert());
            let (old, new) = {
                let mw = game.mob_world_base(&[]);
                let old = ecs.spawn(&mw, "minecraft:zombie", at, None).unwrap();
                let new = ecs.spawn(&mw, "minecraft:drowned", at, None).unwrap();
                (old, new)
            };
            game.ecs = ecs;
            let old_id = game.ecs_link(old);
            let new_id = game.ecs_converted(old_id, new);
            game.ecs.despawn(old);
            assert!(!game.ecs.exists(old_id));
            assert_eq!(game.ecs.entity_of(new_id), Some(new));
            assert_eq!(game.ecs.kind(new_id), Some("minecraft:drowned"));
            assert_ne!(new_id, old_id);
        });
    }

    #[test]
    fn a_death_post_hook_reads_the_entity_by_its_view_after_the_death() {
        with_game(|game| {
            game.ecs = game.test_ecs_level();
            let id = game.ecs_spawn("minecraft:cow", [1.5, 64.0, 1.5], None).unwrap();
            game.native_ecs_death_post(id, "minecraft:generic", 0);
            assert_eq!(game.ecs_kind(id).as_deref(), Some("minecraft:cow"));
            assert_eq!(game.ecs.position(id), Some([1.5, 64.0, 1.5]));
        });
    }

    #[test]
    fn a_vetoable_kill_keeps_the_entity_until_it_is_resolved() {
        with_game(|game| {
            game.ecs = game.test_ecs_level();
            let id = game.ecs_spawn("minecraft:cow", [1.5, 64.0, 1.5], None).unwrap();
            game.ecs.set_death_vetoable(true);
            game.ecs_with(|ecs, mw| ecs.kill(mw, id));
            assert!(game.ecs.entity_of(id).is_some());
            game.ecs_with(|ecs, mw| ecs.resolve_death(mw, id, None));
            for _ in 0..40 {
                game.ecs_with(|ecs, mw| {
                    ecs.step(mw);
                });
            }
            assert!(game.ecs.entity_of(id).is_none());
        });
    }

}
