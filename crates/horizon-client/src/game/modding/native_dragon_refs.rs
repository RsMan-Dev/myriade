//! Ender dragon handles for native server mods: the `Mob` state the dragon
//! keeps, its phase and the fight it belongs to.
use super::{
    Dim, Game,
    commands::Target,
    dragon::{Dragon, Fight},
};
use mod_api::{DragonPhase, DragonRef, EntityRef, LivingRef, MobRef};

impl Game<'_> {
    /// The dragon an entity handle names now.
    pub(super) fn native_dragon(&self, entity: EntityRef) -> Option<&Dragon> {
        let Target::Dragon(id) = self.native_entity_target(entity)? else {
            return None;
        };
        self.dragon.as_ref().filter(|d| d.id == id)
    }
    pub(super) fn native_dragon_mut(&mut self, entity: EntityRef) -> Option<&mut Dragon> {
        let Target::Dragon(id) = self.native_entity_target(entity)? else {
            return None;
        };
        self.dragon.as_mut().filter(|d| d.id == id)
    }

    /// The entity as the ender dragon, or None for another kind, a stale
    /// handle or a client.
    pub(super) fn dragon(&self, entity: EntityRef) -> Option<DragonRef> {
        self.native_dragon(entity)?;
        Some(DragonRef::new(entity))
    }

    /// `EnderDragonPhaseManager.getCurrentPhase`.
    pub(super) fn dragon_phase(&self, dragon: DragonRef) -> Option<DragonPhase> {
        Some(self.native_dragon(dragon.entity())?.native_phase())
    }

    /// `EnderDragonPhaseManager.setPhase`: the dragon ends its phase and
    /// begins the new one fresh (targets and counters cleared), and the
    /// clients get the synced `DATA_PHASE`. Setting the phase it holds does
    /// nothing, as in vanilla (a strafing dragon keeps its player). No
    /// random draw. False for a stale handle or a client. The new phase
    /// runs from the dragon's next tick.
    pub(super) fn dragon_set_phase(&mut self, dragon: DragonRef, phase: DragonPhase) -> bool {
        let Some(d) = self.native_dragon_mut(dragon.entity()) else {
            return false;
        };
        d.set_native_phase(phase);
        self.end_dragon_flame();
        true
    }

    /// The dragon's fight: Horizon keeps the End's fight only, so it is the
    /// fight of a dragon in the level in place when that is the End.
    fn native_dragon_fight(&self, dragon: DragonRef) -> Option<&Fight> {
        self.native_dragon(dragon.entity())?;
        (self.dim == Dim::End && self.fight.loaded).then_some(&self.fight)
    }

    /// `EnderDragonFight.aliveCrystals`: the crystals the last scan found
    /// (every 100 ticks). None for a stale handle, a client, or a dragon
    /// outside the End or whose fight is not loaded.
    pub(super) fn dragon_alive_crystals(&self, dragon: DragonRef) -> Option<i32> {
        Some(self.native_dragon_fight(dragon)?.alive_crystals)
    }

    /// `EnderDragonFight.hasPreviouslyKilledDragon`. None as
    /// `alive_crystals` is.
    pub(super) fn dragon_previously_killed(&self, dragon: DragonRef) -> Option<bool> {
        Some(self.native_dragon_fight(dragon)?.previously_killed)
    }

    /// `EnderDragon.canAttack` (`canBeSeenAsEnemy`) and `Mob.asValidTarget`
    /// for the targets Horizon models: mobs and players. Armor stands, end
    /// crystals and the like are not supported.
    fn native_dragon_valid_target(&self, target: Target) -> bool {
        match target {
            // `Mob.asValidTarget`: alive and not invulnerable (the entity's `Invulnerable` tag).
            Target::Mob(id) if self.ecs.is_active() => {
                self.ecs.alive(id) && !self.ecs_flag(id, "Invulnerable").unwrap_or(false)
            }
            Target::Mob(id) => self
                .remote_entities
                .iter()
                .any(|m| m.id == id && !m.removed && m.alive() && !m.invulnerable_flag),
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
        let target = self.entity(self.native_dragon(entity)?.target?)?;
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
        if self.native_dragon(entity).is_none() {
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
        let Some(dragon) = self.native_dragon_mut(entity) else {
            return false;
        };
        dragon.target = id;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::Dim;
    use super::super::native_entity_refs::tests::with_game;
    use super::*;
    use horizon_core::geometry::Vec3;
    use horizon_core::nbt::Tag;
    use horizon_protocol::entity_data::{Value, serializer};
    use mod_api::{MobRef, WorldEpoch};

    fn dragon_in(game: &mut Game<'_>) -> (EntityRef, DragonRef) {
        let mut dragon = Dragon::new(Vec3::ZERO, 0.0, 0);
        dragon.id = 15;
        game.dragon.replace(dragon);
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
                assert_eq!(game.dragon.as_ref().unwrap().target, None);
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
                let state = game.lan_dragon_state().unwrap();
                let synced = state.meta.iter().find(|m| m.0 == 16);
                if phase == DragonPhase::Hover {
                    assert_eq!(synced, None);
                } else {
                    assert_eq!(synced, Some(&(16, serializer::INT, Value::Int(i as i32))));
                }
                let saved = game.dragon.as_ref().unwrap().to_nbt([0; 4], None);
                assert_eq!(saved.get("DragonPhase"), Some(&Tag::Int(i as i32)));
                let loaded = Dragon::from_nbt(&saved, 0).unwrap();
                assert_eq!(loaded.native_phase(), phase);
            }
            assert_eq!(DragonPhase::from_id(11), None);
            assert_eq!(DragonPhase::from_id(-1), None);

            // Setting the phase it holds changes nothing, a strafe included.
            assert!(dragon.set_phase(game, DragonPhase::Hover));
            assert_eq!(dragon.phase(game), Some(DragonPhase::Hover));

            // The fight: unloaded until the End loads it, and only the End's.
            assert_eq!(dragon.alive_crystals(game), None);
            assert_eq!(dragon.previously_killed(game), None);
            game.fight.loaded = true;
            game.fight.alive_crystals = 3;
            game.fight.previously_killed = true;
            assert_eq!(dragon.alive_crystals(game), None, "not in the End");
            assert_eq!(dragon.previously_killed(game), None);
            game.dim = Dim::End;
            // This fixture relocates the level in place rather than loading
            // another one. Rebind its owners to the new dimension as well.
            game.reset_entity_residency();
            assert_eq!(dragon.alive_crystals(game), Some(3));
            assert_eq!(dragon.previously_killed(game), Some(true));

            let stale = DragonRef::new(EntityRef::new(
                WorldEpoch::new(entity.epoch().get() + 1),
                15,
            ));
            assert!(!stale.set_phase(game, DragonPhase::Landing));
            assert_eq!(stale.phase(game), None);
            assert_eq!(stale.alive_crystals(game), None);
            game.dragon.discard();
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

    /// Part `i` of the test dragon: a position nothing else shares and, from
    /// vanilla's `EnderDragon` constructor, the part's width and height.
    fn part_fixture(i: usize) -> ([f64; 3], [f64; 2]) {
        const SIZES: [[f64; 2]; 8] = [
            [1.0, 1.0],
            [3.0, 3.0],
            [5.0, 3.0],
            [2.0, 2.0],
            [2.0, 2.0],
            [2.0, 2.0],
            [4.0, 2.0],
            [4.0, 2.0],
        ];
        let i_f = i as f64;
        (
            [100.0 + 10.0 * i_f, 70.0 + i_f, -20.0 + 0.5 * i_f],
            SIZES[i],
        )
    }

    fn place_parts(game: &mut Game<'_>) {
        for i in 0..8 {
            let [x, y, z] = part_fixture(i).0;
            game.dragon.as_mut().unwrap().parts[i].0 = Vec3::new(x, y, z);
        }
    }

    fn assert_parts_are_the_dragons_own(game: &Game<'_>, dragon: EntityRef) {
        let parts = game.entity_parts(dragon).unwrap();
        let ids: Vec<u32> = parts.iter().map(|part| part.id()).collect();
        assert_eq!(ids, (dragon.id() + 1..=dragon.id() + 8).collect::<Vec<_>>());
        assert_eq!(game.entity_is_multipart(dragon), Some(true));
        for (i, part) in parts.iter().enumerate() {
            let ([x, y, z], [width, height]) = part_fixture(i);
            // The handle `entity` gives is the one `parts` listed.
            assert_eq!(game.entity(part.id()), Some(*part));
            assert_ne!(*part, dragon);
            assert_eq!(game.entity_position(*part), Some([x, y, z]));
            assert_eq!(
                game.entity_bounding_box(*part),
                Some([
                    [x - width / 2.0, y, z - width / 2.0],
                    [x + width / 2.0, y + height, z + width / 2.0]
                ])
            );
            assert_eq!(
                game.entity_kind(*part).map(|kind| kind.id(game)).as_deref(),
                Some("minecraft:ender_dragon")
            );
            assert_eq!(game.entity_rotation(*part), Some([0.0, 0.0]));
            assert_eq!(game.entity_velocity(*part), Some([0.0; 3]));
            assert_eq!(game.entity_level(*part), game.entity_level(dragon));
            assert_eq!(game.entity_is_multipart(*part), Some(false));
            assert_eq!(game.entity_parts(*part), Some(Vec::new()));
            // Its rotation is none, so what is up for it is up.
            assert!(game.entity_up_vector(*part).unwrap()[1] > 0.999_999);
            // No living entity, mob or dragon behind it.
            assert!(game.living(*part).is_none());
            assert!(game.mob(*part).is_none());
            assert!(game.dragon(*part).is_none());
        }
        // The parent keeps its own place and has no box.
        assert_eq!(game.entity_position(dragon), Some([0.0; 3]));
        assert_eq!(game.entity_bounding_box(dragon), None);
    }

    #[test]
    #[ignore = "daily"]
    fn the_dragons_parts_resolve_as_entities_of_their_own() {
        with_game(|game| {
            let (entity, _) = dragon_in(game);
            place_parts(game);
            assert_parts_are_the_dragons_own(game, entity);
            let parts = game.entity_parts(entity).unwrap();
            // Only the eight ids after the dragon's are its parts.
            assert!(game.entity(entity.id() - 1).is_none());
            assert!(game.entity(entity.id() + 9).is_none());
            // A part cannot leave alone.
            assert!(!game.entity_discard(parts[0]));
            assert!(
                !game.entity_remove(parts[7], mod_api::RemovalReason::Killed)
                    && game.entity(parts[7].id()).is_some()
            );
            // A handle of an earlier open names nothing.
            let stale = EntityRef::new(WorldEpoch::new(entity.epoch().get() + 1), parts[0].id());
            assert_eq!(game.entity_position(stale), None);
            assert_eq!(game.entity_bounding_box(stale), None);
            assert_eq!(game.entity_is_multipart(stale), None);
            let stale_dragon = EntityRef::new(stale.epoch(), entity.id());
            assert_eq!(game.entity_parts(stale_dragon), None);
            // Parts cannot move independently of their parent, even without rotation.
            let before = game.entity_position(parts[2]);
            assert!(!game.entity_teleport_to(parts[2], [10.0, 80.0, 10.0], None));
            assert_eq!(game.entity_position(parts[2]), before);
            // A dragon that is dying still has its parts.
            game.dragon.as_mut().unwrap().health = 0.0;
            assert_eq!(game.entity_position(parts[2]), Some(part_fixture(2).0));
            // A dragon a mod removed takes its parts with it, before the sweep.
            assert!(game.entity_discard(entity));
            for part in &parts {
                assert_eq!(game.entity_position(*part), None);
                assert!(game.entity(part.id()).is_none());
            }
            assert_eq!(game.entity_parts(entity), None);
            game.native_removals_flush();
            assert!(game.dragon.is_none());
            for part in &parts {
                assert_eq!(game.entity_bounding_box(*part), None);
            }
        });
    }

    #[test]
    #[ignore = "daily"]
    fn a_dragon_that_has_no_id_yet_has_no_parts() {
        with_game(|game| {
            game.dragon.replace(Dragon::new(Vec3::ZERO, 0.0, 0));
            // The ids after 0 are the player's and its neighbours', not parts.
            for id in 1..=8 {
                assert!(game.level_dragon_part(id).is_none());
            }
        });
    }
}
