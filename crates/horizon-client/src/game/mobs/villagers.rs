//! What villagers do with each other and with the player: gossip and
//! reputation, sharing food and wares, picking up dropped items, making
//! babies, calling iron golems and the prices they ask. What stays here
//! needs the game itself: the particles, the player UUIDs, the merchant menu, the
//! sleep hooks and the wake-up inside a mob pass.
use super::Game;
#[cfg(test)]
use horizon_client::mobs::{Mob, villager, villager_brain};
#[cfg(test)]
use horizon_core::geometry::Vec3;

impl Game<'_> {
    /// `Level.broadcastEntityEvent` of an event whose `handleEntityEvent`
    /// draws particles about the mob (villager 12, 13 and 14, taming 6 and 7,
    /// love 18): the guests get the byte.
    pub(super) fn mob_event_particles(&mut self, id: u32, event: i8) {
        let Some(m) = self.remote_entities.iter().find(|m| m.id == id) else {
            return;
        };
        let at = [m.position.x, m.position.y, m.position.z];
        self.lan_entity_event(id as i32, event, at);
    }
    /// The UUID of a player by connection (None for the host).
    pub(super) fn player_uuid_of(&self, who: Option<u32>) -> Option<[i32; 4]> {
        let uuid = match who {
            None => return self.level.player_uuid,
            // While that guest acts its own UUID is swapped in.
            Some(conn) if self.lan_acting == Some(conn) => self.lan_acting_uuid?,
            Some(conn) => self.lan.as_ref()?.hub.guest(conn)?.uuid,
        };
        Some(horizon_client::lan::playerdata::uuid_ints(uuid))
    }
    /// The Hero of the Village amplifier of the player, if any.
    pub(crate) fn hero_amplifier(&self) -> Option<i32> {
        self.potions.effects.amplifier("hero_of_the_village")
    }
}

#[cfg(test)]
mod entity_event_particle_tests {
    /// The raid sweat (42) and the dolphin's treasure hearts (38) are in the
    /// table that now decides which entity events draw particles.
    #[test]
    fn the_table_draws_the_raid_sweat_and_the_dolphin_event() {
        let kind = |entity, event| {
            horizon_client::particles::entity_event_burst(entity, event).map(|(k, _)| k)
        };
        assert_eq!(kind("villager", 42), Some("splash"));
        assert_eq!(kind("dolphin", 38), Some("happy_villager"));
        assert_eq!(kind("villager", 2), None);
    }
}

#[cfg(test)]
mod nested_spawn_tests {
    #[test]
    #[ignore = "daily"]
    fn lifted_mob_callback_keeps_nested_spawns_until_the_next_pass() {
        crate::game::entity::taken_pass_tests::in_both_dimensions(|game, balance| {
            let kind = game
                .res
                .mob_types()
                .get("minecraft:villager")
                .unwrap()
                .clone();
            game.mobs.push(horizon_client::mobs::Mob::new(
                99,
                kind,
                horizon_core::geometry::Vec3::new(0.5, 80.0, 0.5),
                0.0,
            ));
            crate::game::entity::taken_pass_tests::assert_mob_spawn_survives(
                game,
                balance,
                |game| {
                    game.wake_villager(99);
                },
            );
        });
    }
}

#[cfg(test)]
mod off_level_tests {
    use super::*;

    #[test]
    #[ignore = "daily"]
    fn retained_ancestor_does_not_consume_a_due_villager_sensor() {
        crate::game::native_entity_refs::tests::with_game(|game| {
            game.options.dedicated = Some(Vec::new());
            game.world.chunks.insert(
                (0, 0),
                std::sync::Arc::new(horizon_client::chunk::Chunk::empty(0, 0)),
            );
            let kind = game
                .res
                .mob_types()
                .get("minecraft:villager")
                .unwrap()
                .clone();
            let mut m = Mob::new(73, kind.clone(), Vec3::new(1.0, 80.0, 0.5), 0.0);
            villager_brain::init(&mut m, 0, 0, 0);
            let mut health = horizon_client::health::TestLevel::new(0);
            for _ in 0..40 {
                villager_brain::tick(
                    &mut m,
                    &game.mob_world_base(&[]),
                    &mut health.level(),
                    &mut Vec::new(),
                );
            }
            m.extra.mount = Some(72);
            game.mobs.push(m);
            let ghost = Mob::new(72, kind, Vec3::ZERO, 0.0);
            game.persist
                .off_level
                .insert(72, crate::game::persist::OffLevel::Mob(ghost));
            game.tick_creature_entities().unwrap();
            assert!(
                villager_brain::take_item_scan(game.mob_object_mut(73).unwrap()),
                "a frozen resident's due sensor was consumed by independent item sensing"
            );
        });
    }
}

#[cfg(test)]
mod birth_tests {
    use super::*;
    use horizon_client::mobs::MobEvent;
    use mod_api::{
        BabyEntitySpawnEvent, EntityJoinLevelEvent, FinalizeSpawnEvent, Flow, Hooks, Host,
        ServerHost,
    };
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    const BED: [i32; 3] = [3, 64, 1];

    fn parents(game: &mut Game<'_>) -> (u32, u32) {
        game.mobs.clear();
        game.world
            .chunks
            .insert((0, 0), Arc::new(horizon_client::chunk::Chunk::empty(0, 0)));
        let bed = game
            .pack
            .assets
            .registry
            .resolve(&horizon_world::chunk::BlockState {
                name: "minecraft:red_bed".into(),
                properties: [
                    ("facing".into(), "east".into()),
                    ("occupied".into(), "false".into()),
                    ("part".into(), "head".into()),
                ]
                .into_iter()
                .collect(),
            })
            .unwrap();
        game.world.set(BED[0], BED[1], BED[2], bed).unwrap();
        let kind = game
            .res
            .mob_types()
            .get("minecraft:villager")
            .unwrap()
            .clone();
        let parent = game.next_entity + 1;
        let partner = parent + 1;
        game.next_entity = partner;
        for id in [parent, partner] {
            game.mobs.push(Mob::with_random(
                id,
                kind.clone(),
                Vec3::new(1.5, 64.0, 1.5),
                0.0,
                horizon_core::random::EntityRandom::own(horizon_core::random::LegacyRandom::new(
                    i64::from(id),
                )),
            ));
        }
        // The brain has already eaten, selected BED and aged the initiating
        // parent before emitting VillagerBirth; the Game owns the other half.
        game.mobs[0].extra.age = 6000;
        game.native_joins_skip();
        game.audio.capture = Some(Vec::new());
        game.lan_entity_events.clear();
        (parent, partner)
    }

    fn birth(game: &mut Game<'_>, parent: u32, partner: u32) {
        game.handle_mob_events(vec![MobEvent::VillagerBirth {
            parent,
            partner,
            child: Some((BED, Some(2))),
        }])
        .unwrap();
        for id in [parent, partner] {
            assert_eq!(
                game.mobs.iter().find(|m| m.id == id).unwrap().extra.age,
                6000
            );
        }
    }

    fn bed_owners(game: &Game<'_>) -> Vec<u32> {
        // The same Seen.bed projection supplies the POI ticket searches and
        // take_bed_for_child; no independent reservation exists in the Game.
        game.mobs
            .iter()
            .filter(|m| m.seen(None, game.items()).bed == Some(BED))
            .map(|m| m.id)
            .collect()
    }

    struct RejectJoin(Arc<AtomicUsize>);
    #[mod_api::__subscriptions]
    impl Hooks for RejectJoin {
        fn entity_join_level(
            &self,
            _: &mut dyn ServerHost,
            event: &mut EntityJoinLevelEvent<'_>,
        ) -> Flow {
            assert_eq!(event.kind, "minecraft:villager");
            self.0.fetch_add(1, Ordering::SeqCst);
            Flow::Cancel
        }
    }

    #[test]
    #[ignore = "daily"]
    fn rejected_birth_leaves_bed_free_and_parents_resting() {
        let joins = Arc::new(AtomicUsize::new(0));
        let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
            "reject-villager-birth",
            Box::new(RejectJoin(joins.clone())),
        )]
        .into();
        mod_platform::testing::with_mods(mods, || {
            crate::game::native_entity_refs::tests::with_game(|game| {
                let (parent, partner) = parents(game);
                assert!(bed_owners(game).is_empty());
                birth(game, parent, partner);
                assert_eq!(joins.load(Ordering::SeqCst), 1);
                assert_eq!(game.mobs.len(), 2);
                assert!(bed_owners(game).is_empty());
                assert!(game.lan_entity_events.is_empty());
            });
        });
    }

    fn assert_baby(game: &Game<'_>, parent: u32, partner: u32) {
        assert_eq!(game.mobs.len(), 3);
        let baby = game
            .mobs
            .iter()
            .find(|m| m.id != parent && m.id != partner)
            .unwrap();
        assert_eq!(baby.extra.age, -24000);
        assert_eq!(baby.extra.variant, 2);
        assert_eq!(baby.extra.bed, Some(BED));
        assert_eq!(bed_owners(game), vec![baby.id]);
        assert_eq!(game.lan_entity_events, vec![(baby.id as i32, 12)]);
    }

    #[test]
    #[ignore = "daily"]
    fn untouched_birth_claims_one_bed_and_announces_one_baby() {
        mod_platform::testing::with_mods(Vec::<mod_api::ModEntry>::new().into(), || {
            crate::game::native_entity_refs::tests::with_game(|game| {
                let (parent, partner) = parents(game);
                assert!(bed_owners(game).is_empty());
                birth(game, parent, partner);
                assert_baby(game, parent, partner);
            });
        });
    }

    struct RejectOtherBirthPaths(Arc<AtomicUsize>);
    #[mod_api::__subscriptions]
    impl Hooks for RejectOtherBirthPaths {
        fn finalize_spawn(&self, _: &dyn Host, event: &mut FinalizeSpawnEvent<'_>) -> Flow {
            self.0.fetch_add(1, Ordering::SeqCst);
            event.spawn_cancelled = true;
            Flow::Continue
        }

        fn baby_entity_spawn(
            &self,
            _: &mut dyn ServerHost,
            _: &mut BabyEntitySpawnEvent<'_>,
        ) -> Flow {
            self.0.fetch_add(1, Ordering::SeqCst);
            Flow::Cancel
        }
    }

    #[test]
    #[ignore = "daily"]
    fn villager_birth_is_raw_and_does_not_post_the_animal_hook() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
            "reject-non-villager-birth-paths",
            Box::new(RejectOtherBirthPaths(calls.clone())),
        )]
        .into();
        mod_platform::testing::with_mods(mods, || {
            crate::game::native_entity_refs::tests::with_game(|game| {
                let (parent, partner) = parents(game);
                birth(game, parent, partner);
                assert_baby(game, parent, partner);
                assert_eq!(calls.load(Ordering::SeqCst), 0);
            });
        });
    }
}
