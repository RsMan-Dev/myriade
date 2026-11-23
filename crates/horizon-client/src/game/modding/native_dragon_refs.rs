//! Ender dragon handles for native server mods: the ECS dragon they name,
//! its phase, its target and the fight it belongs to.
use super::{Dim, Game, commands::Target};
use horizon_ecs_proto::bridge::dragon::DragonView;
use mod_api::{DragonPhase, DragonRef, EntityRef, LivingRef, MobRef};

impl Game<'_> {
    /// The ECS dragon an entity handle names now, with its network id.
    fn native_dragon_view(&self, entity: EntityRef) -> Option<(u32, DragonView)> {
        let Target::Mob(id) = self.native_entity_target(entity)? else {
            return None;
        };
        self.ecs.dragon_views().into_iter().find(|(d, _)| *d == id)
    }

    /// The entity as the ender dragon, or None for another kind, a stale
    /// handle or a client.
    pub(super) fn dragon(&self, entity: EntityRef) -> Option<DragonRef> {
        self.native_dragon_view(entity)?;
        Some(DragonRef::new(entity))
    }

    /// `EnderDragonPhaseManager.getCurrentPhase`.
    pub(super) fn dragon_phase(&self, dragon: DragonRef) -> Option<DragonPhase> {
        DragonPhase::from_id(i32::from(self.native_dragon_view(dragon.entity())?.1.phase))
    }

    /// `EnderDragonPhaseManager.setPhase`: the dragon ends its phase and
    /// begins the new one fresh, in the ECS. Setting the phase it holds does
    /// nothing, as in vanilla. No random draw. False for a stale handle or a
    /// client. The new phase runs from the dragon's next tick.
    pub(super) fn dragon_set_phase(&mut self, dragon: DragonRef, phase: DragonPhase) -> bool {
        let Some((id, _)) = self.native_dragon_view(dragon.entity()) else {
            return false;
        };
        self.ecs_with(|ecs, mw| ecs.set_dragon_phase(mw, id, phase.id()))
    }

    /// The dragon's fight, as (alive crystals, previously killed): Horizon
    /// keeps the End's fight only, so it is the fight of a dragon in the level
    /// in place when that is the End and its fight is loaded.
    fn native_dragon_fight(&self, dragon: DragonRef) -> Option<(u32, bool)> {
        self.native_dragon_view(dragon.entity())?;
        let fight = self.ecs.fight_view();
        (self.dim == Dim::End && fight.loaded)
            .then_some((fight.crystals_alive, fight.previously_killed))
    }

    /// `EnderDragonFight.aliveCrystals`: the crystals the last scan found
    /// (every 100 ticks). None for a stale handle, a client, or a dragon
    /// outside the End or whose fight is not loaded.
    pub(super) fn dragon_alive_crystals(&self, dragon: DragonRef) -> Option<i32> {
        Some(self.native_dragon_fight(dragon)?.0 as i32)
    }

    /// `EnderDragonFight.hasPreviouslyKilledDragon`. None as
    /// `alive_crystals` is.
    pub(super) fn dragon_previously_killed(&self, dragon: DragonRef) -> Option<bool> {
        Some(self.native_dragon_fight(dragon)?.1)
    }

    /// `EnderDragon.canAttack` (`canBeSeenAsEnemy`) and `Mob.asValidTarget`
    /// for the targets Horizon models: mobs and players. Armor stands, end
    /// crystals and the like are not supported.
    fn native_dragon_valid_target(&self, target: Target) -> bool {
        match target {
            // `Mob.asValidTarget`: alive and not invulnerable (the entity's `Invulnerable` tag).
            Target::Mob(id) => {
                self.ecs.alive(id) && !self.ecs_flag(id, "Invulnerable").unwrap_or(false)
            }
            Target::Player | Target::Guest(_) => self
                .native_player_facts(target)
                .is_some_and(|p| p.alive && !p.invulnerable && !matches!(p.mode, 1 | 3)),
            _ => false,
        }
    }

    /// `Mob.getTarget` for the dragon: the mob or player a mod set, None once
    /// it is no longer a valid target. The dragon's flight never sets or
    /// reads it.
    pub(super) fn native_dragon_target(&self, entity: EntityRef) -> Option<LivingRef> {
        let target = self.entity(self.native_dragon_view(entity)?.1.target?)?;
        self.native_dragon_valid_target(self.native_entity_target(target)?)
            .then(|| self.living(target))?
    }

    /// `Mob.setTarget` for the dragon; None clears it. An invalid target is
    /// refused (false, unchanged) where vanilla stores None. False too for a
    /// stale handle or a target that is not a mob or a player.
    pub(super) fn native_dragon_set_target(
        &mut self,
        entity: EntityRef,
        target: Option<LivingRef>,
    ) -> bool {
        if self.native_dragon_view(entity).is_none() {
            return false;
        }
        if let Some(target) = target
            && !self
                .native_entity_target(target.entity())
                .is_some_and(|t| self.native_dragon_valid_target(t))
        {
            return false;
        }
        let Some(target) = self.native_mob_target_event(MobRef::new(entity), target, false) else {
            return false;
        };
        let id = match target {
            None => None,
            Some(target) => {
                let valid = self
                    .native_entity_target(target.entity())
                    .is_some_and(|t| self.native_dragon_valid_target(t));
                if !valid {
                    return false;
                }
                Some(target.entity().id())
            }
        };
        let Some((dragon, _)) = self.native_dragon_view(entity) else {
            return false;
        };
        self.ecs.set_dragon_target(dragon, id)
    }
}

#[cfg(test)]
mod tests {
    use super::super::Dim;
    use super::super::native_entity_refs::tests::with_game;
    use super::*;
    use horizon_core::geometry::Vec3;
    use mod_api::WorldEpoch;

    fn dragon_in(game: &mut Game<'_>) -> (EntityRef, DragonRef) {
        game.ecs_with(|ecs, mw| {
            ecs.summon_dragon(
                mw,
                [0.0, 80.0, 0.0],
                0.0,
                0,
                horizon_ecs_proto::species::ender_dragon::START_PHASE,
                &horizon_ecs_proto::spawn_sheet::SpawnSheet::default(),
                &mut || 15,
            )
        });
        let entity = game.entity(15).unwrap();
        (entity, game.dragon(entity).unwrap())
    }

    #[test]
    #[ignore = "daily"]
    fn dragon_target_change_can_be_cancelled() {
        struct Cancel;
        #[mod_api::__subscriptions]
        impl mod_api::Hooks for Cancel {
            fn living_change_target(
                &self,
                host: &dyn mod_api::Host,
                event: &mut mod_api::LivingChangeTargetEvent<'_>,
            ) -> mod_api::Flow {
                assert!(event.mob.living().health(host).is_some());
                mod_api::Flow::Cancel
            }
        }
        let mods = vec![mod_api::ModEntry::new("cancel-target", Box::new(Cancel))].into();
        mod_platform::testing::with_mods(mods, || {
            with_game(|game| {
                let (_, dragon) = dragon_in(game);
                assert!(!dragon.mob().set_target(game, None));
                assert_eq!(dragon.mob().target(game), None);
            })
        });
    }

    #[test]
    #[ignore = "daily"]
    fn the_phase_is_read_set_synced_and_saved() {
        with_game(|game| {
            let (entity, dragon) = dragon_in(game);
            // A constructed dragon (as with /summon) hovers; only the fight's
            // create_new_dragon switches it to the holding pattern.
            assert_eq!(dragon.phase(game), Some(DragonPhase::Hover));
            for (i, phase) in [
                DragonPhase::HoldingPattern,
                DragonPhase::StrafePlayer,
                DragonPhase::LandingApproach,
                DragonPhase::Landing,
                DragonPhase::Takeoff,
                DragonPhase::SittingFlaming,
                DragonPhase::SittingScanning,
                DragonPhase::SittingAttacking,
                DragonPhase::ChargingPlayer,
                DragonPhase::Dying,
                DragonPhase::Hover,
            ]
            .into_iter()
            .enumerate()
            {
                assert_eq!(phase.id(), i as i32);
                assert_eq!(DragonPhase::from_id(i as i32), Some(phase));
                assert!(dragon.set_phase(game, phase));
                assert_eq!(dragon.phase(game), Some(phase));
            }
            assert_eq!(DragonPhase::from_id(11), None);
            assert_eq!(DragonPhase::from_id(-1), None);

            // Setting the phase it holds changes nothing, a strafe included.
            assert!(dragon.set_phase(game, DragonPhase::Hover));
            assert_eq!(dragon.phase(game), Some(DragonPhase::Hover));

            // The fight: unloaded until the End loads it, and only the End's.
            assert_eq!(dragon.alive_crystals(game), None);
            assert_eq!(dragon.previously_killed(game), None);
            game.dim = Dim::End;
            // This fixture relocates the level in place rather than loading
            // another one. Rebind its owners to the new dimension as well.
            game.reset_entity_residency();
            assert_eq!(dragon.alive_crystals(game), None, "the fight is not loaded");
            assert_eq!(dragon.previously_killed(game), None);

            let stale = DragonRef::new(EntityRef::new(
                WorldEpoch::new(entity.epoch().get() + 1),
                15,
            ));
            assert!(!stale.set_phase(game, DragonPhase::Landing));
            assert_eq!(stale.phase(game), None);
            assert_eq!(stale.alive_crystals(game), None);
            assert!(game.entity_discard(entity));
            assert_eq!(dragon.phase(game), None);
            assert!(!dragon.set_phase(game, DragonPhase::Landing));
        });
    }

    #[test]
    #[ignore = "daily"]
    fn an_invulnerable_player_is_not_a_target() {
        with_game(|game| {
            let (entity, _) = dragon_in(game);
            let mob = game.mob(entity).unwrap();
            game.player =
                Some(horizon_client::motion::PlayerMotion::new(game.motion, Vec3::ZERO).unwrap());
            crate::game::native_entity_refs::tests::bind_local_player(game, 30, 7);
            let target = game.entity(30).unwrap();
            let living = game.living(target).unwrap();
            assert!(mob.set_target(game, Some(living)));
            game.player.as_mut().unwrap().invulnerable = true;
            assert_eq!(mob.target(game), None);
            assert!(!mob.set_target(game, Some(living)));
        });
    }

    #[test]
    #[ignore = "daily"]
    fn the_dragons_parts_resolve_as_entities_of_their_own() {
        with_game(|game| {
            let (entity, _) = dragon_in(game);
            let parts = game.entity_parts(entity).unwrap();
            let ids: Vec<u32> = parts.iter().map(|part| part.id()).collect();
            assert_eq!(ids, (entity.id() + 1..=entity.id() + 8).collect::<Vec<_>>());
            assert_eq!(game.entity_is_multipart(entity), Some(true));
            for part in &parts {
                // The handle `entity` gives is the one `parts` listed.
                assert_eq!(game.entity(part.id()), Some(*part));
                assert_ne!(*part, entity);
                assert_eq!(
                    game.entity_kind(*part).map(|kind| kind.id(game)).as_deref(),
                    Some("minecraft:ender_dragon")
                );
                assert_eq!(game.entity_is_multipart(*part), Some(false));
                // No living entity, mob or dragon behind it.
                assert!(game.living(*part).is_none());
                assert!(game.mob(*part).is_none());
                assert!(game.dragon(*part).is_none());
            }
            // Only the eight ids after the dragon's are its parts.
            assert!(game.entity(entity.id() - 1).is_none());
            assert!(game.entity(entity.id() + 9).is_none());
            // A part cannot leave alone.
            assert!(!game.entity_discard(parts[0]));
            // A handle of an earlier open names nothing.
            let stale = EntityRef::new(WorldEpoch::new(entity.epoch().get() + 1), parts[0].id());
            assert_eq!(game.entity_position(stale), None);
            assert_eq!(game.entity_bounding_box(stale), None);
            // Parts cannot move independently of their parent.
            assert!(!game.entity_teleport_to(parts[2], [10.0, 80.0, 10.0], None));
            // A dragon a mod removed takes its parts with it, before the sweep.
            assert!(game.entity_discard(entity));
            for part in &parts {
                assert_eq!(game.entity_position(*part), None);
                assert!(game.entity(part.id()).is_none());
            }
            assert_eq!(game.entity_parts(entity), None);
            game.native_removals_flush();
            for part in &parts {
                assert_eq!(game.entity_bounding_box(*part), None);
            }
        });
    }

    #[test]
    #[ignore = "daily"]
    fn a_dragon_that_has_no_id_yet_has_no_parts() {
        with_game(|game| {
            // The ids after 0 are the player's and its neighbours', not parts.
            for id in 1..=8 {
                assert!(game.level_dragon_part(id).is_none());
            }
        });
    }
}
