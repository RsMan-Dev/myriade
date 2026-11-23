use super::*;

/// The hook points. Each does nothing in Horizon.
impl Game<'_> {
    /// `MobSpawnEvent.SpawnPlacementCheck`: the entity type `kind` may spawn
    /// at `pos` in `level` for `reason` (light, ground, biome:
    /// `SpawnPlacements.checkSpawnRules`). `default_result` is what vanilla's
    /// predicate answered; `spawner` is the block that asked, if one did;
    /// `result` starts as `Default` (vanilla's answer stands). Server-side.
    pub(in crate::game) fn native_mob_spawn_placement_check(
        &self,
        call: mod_api::__private::MobSpawnPlacementCheckCall<'_>,
    ) {
        self.fire_mob_spawn_placement_check(call);
    }
    /// `MobSpawnEvent.PositionCheck`: the freshly created mob of type `kind`
    /// may stand at `pos` in `level` for `reason` (`Mob.checkSpawnRules` and
    /// `checkSpawnObstruction`). `spawner` is the block that asked, if one
    /// did; `mob` is the mob itself, lent for the call. `result` starts as
    /// `Default`: vanilla's two checks decide; `Succeed` or `Fail` skips them.
    /// Server-side.
    pub(in crate::game) fn native_mob_spawn_position_check(
        &self,
        call: mod_api::__private::MobSpawnPositionCheckCall<'_>,
    ) {
        self.fire_mob_spawn_position_check(call);
    }
}

/// The natural pass's spawn hooks, answered by the game for one step (lent to the ECS while a mod listens to one of them).
pub(in crate::game) struct EcsSpawnHooks<'g, 'a> {
    pub(in crate::game) game: &'g Game<'a>,
}
impl horizon_ecs_proto::spawn_mod_host::SpawnModHost for EcsSpawnHooks<'_, '_> {
    fn placement_check(&self, kind: &str, pos: [i32; 3], reason: horizon_ecs_proto::spawn_reason::SpawnReason, default: bool) -> bool {
        if !mod_api::__private::listened(mod_api::HookId::MobSpawnPlacementCheck) {
            return default;
        }
        let g = self.game;
        let mut result = mod_api::SpawnCheck::Default;
        g.native_mob_spawn_placement_check(mod_api::__private::MobSpawnPlacementCheckCall {
            dimension: g.dim.id(),
            kind,
            reason: game_reason(reason),
            pos: horizon_core::position::BlockPos::new(pos[0], pos[1], pos[2]),
            default_result: default,
            level: g.level_ref(g.lend_epoch(), g.dim.key()),
            spawner: None,
            result: &mut result,
        });
        match result {
            mod_api::SpawnCheck::Succeed => true,
            mod_api::SpawnCheck::Fail => false,
            _ => default,
        }
    }
    fn position_check(&self, kind: &str, pos: [f64; 3], reason: horizon_ecs_proto::spawn_reason::SpawnReason) -> Option<bool> {
        if !mod_api::__private::listened(mod_api::HookId::MobSpawnPositionCheck) {
            return None;
        }
        let g = self.game;
        let level = g.level_ref(g.lend_epoch(), g.dim.key());
        let mut result = mod_api::SpawnCheck::Default;
        g.lend_unspawned(kind, pos, None, |mob, _| {
            g.native_mob_spawn_position_check(mod_api::__private::MobSpawnPositionCheckCall {
                dimension: g.dim.id(),
                kind,
                reason: game_reason(reason),
                pos,
                level,
                spawner: None,
                mob,
                result: &mut result,
            });
        });
        match result {
            mod_api::SpawnCheck::Succeed => Some(true),
            mod_api::SpawnCheck::Fail => Some(false),
            _ => None,
        }
    }
    fn finalize_spawn(
        &self,
        kind: &str,
        pos: [f64; 3],
        reason: horizon_ecs_proto::spawn_reason::SpawnReason,
        group: &mut Option<horizon_ecs_proto::spawn_finalize::Group>,
    ) -> horizon_ecs_proto::spawn_mod_host::Verdict {
        use crate::game::creatures::Finalized;
        use horizon_ecs_proto::spawn_mod_host::Verdict;
        let mut sheet = horizon_ecs_proto::spawn_sheet::SpawnSheet { group: group.take(), ..Default::default() };
        let (done, difficulty) = self.game.finalize_spawn_at(kind, pos, game_reason(reason), None, Some(&mut sheet));
        *group = sheet.group;
        match done {
            Finalized::Cancelled => Verdict::Cancelled,
            Finalized::Skipped => Verdict::Skipped,
            Finalized::Done => Verdict::Done(Some(difficulty.special())),
        }
    }
}
/// The client's spawn reason of an ECS spawn reason (the inverse of `ecs_reason` in `game/ecs/spawn.rs`).
fn game_reason(reason: horizon_ecs_proto::spawn_reason::SpawnReason) -> horizon_client::mobs::spawn_rules::Reason {
    use horizon_client::mobs::spawn_rules::Reason as R;
    use horizon_ecs_proto::spawn_reason::SpawnReason as E;
    match reason {
        E::Natural => R::Natural,
        E::ChunkGeneration => R::ChunkGeneration,
        E::Spawner => R::Spawner,
        E::Structure => R::Structure,
        E::Breeding => R::Breeding,
        E::MobSummoned => R::MobSummoned,
        E::Jockey => R::Jockey,
        E::Event => R::Event,
        E::Conversion => R::Conversion,
        E::Reinforcement => R::Reinforcement,
        E::Triggered => R::Triggered,
        E::Bucket => R::Bucket,
        E::SpawnItemUse => R::SpawnItemUse,
        E::Command => R::Command,
        E::Dispenser => R::Dispenser,
        E::Patrol => R::Patrol,
        E::TrialSpawner => R::TrialSpawner,
        E::Load => R::Load,
        E::DimensionTravel => R::DimensionTravel,
    }
}

impl Game<'_> {
    /// The spawn hooks of the natural pass, when a mod listens to one of them: None otherwise (the pass asks nothing).
    pub(in crate::game) fn ecs_spawn_hooks(&self) -> Option<EcsSpawnHooks<'_, '_>> {
        let listens = [mod_api::HookId::MobSpawnPlacementCheck, mod_api::HookId::MobSpawnPositionCheck, mod_api::HookId::FinalizeSpawn]
            .into_iter()
            .any(mod_api::__private::listened);
        listens.then_some(EcsSpawnHooks { game: self })
    }
}
