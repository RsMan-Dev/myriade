#[cfg(test)]
mod tests {
    use super::super::super::{
        native_entity_refs::tests::with_client_game, native_lifecycle::NativeLifecycle,
    };
    use horizon_client::level_loading::Reason;
    use horizon_protocol::{connection::Pipe, packets::play::SpawnInfo};
    use mod_api::{Hooks, HostSide, LevelRef, LevelTickPostEvent, LevelTickPreEvent, ModEntry};
    use std::sync::{Arc, Mutex};

    #[test]
    #[ignore = "daily"]
    fn server_hook_resolves_uuid_only_while_the_entity_is_in_its_active_level() {
        use super::super::super::{Dim, native_entity_refs::tests::with_game};
        use horizon_client::mobs::Mob;
        use horizon_core::geometry::Vec3;
        use mod_api::EntityUuid;

        const UUID: EntityUuid = EntityUuid(0x0123_4567_89ab_cdef_1020_3040_ffff_ffff);

        type Hit = Option<(u32, [f64; 3])>;

        struct Lookup(Arc<Mutex<Vec<Hit>>>);

        #[mod_api::__subscriptions]
        impl Hooks for Lookup {
            fn level_tick_pre(&self, host: HostSide, _: &mut LevelTickPreEvent) {
                let HostSide::Server(host) = host else {
                    return;
                };
                assert_eq!(host.entity_by_uuid(EntityUuid(u128::MAX)), None);
                let hit = host
                    .entity_by_uuid(UUID)
                    .map(|entity| (entity.id(), entity.position(host).unwrap()));
                self.0.lock().unwrap().push(hit);
            }
        }

        let heard = Arc::new(Mutex::new(Vec::new()));
        let mods = vec![ModEntry::new(
            "uuid_lookup",
            Box::new(Lookup(heard.clone())),
        )]
        .into();
        mod_platform::testing::with_mods(mods, || {
            with_game(|game| {
                let _phase =
                    mod_api::__private::PhaseOwner::scope(mod_api::__private::ModPhase::World);
                let kind = game.res.mob_types().get("minecraft:pig").unwrap().clone();
                game.mobs
                    .push(Mob::new(91, kind.clone(), Vec3::new(3.5, 70.0, 5.5), 0.0));
                game.mobs.push(Mob::new(92, kind, Vec3::ZERO, 0.0));
                game.persist
                    .uuids
                    .insert(91, [0x0123_4567, 0x89ab_cdefu32 as i32, 0x1020_3040, -1]);
                let home = game.level("minecraft:overworld").unwrap();

                let uuids_before = game.persist.uuids.clone();
                game.native_level_tick_pre(home);
                assert_eq!(*game.persist.uuids, uuids_before);

                game.enter_level(Dim::Nether).unwrap();
                let nether = game.level("minecraft:the_nether").unwrap();
                let uuids_before = game.persist.uuids.clone();
                game.native_level_tick_pre(nether);
                assert_eq!(game.dim, Dim::Nether);
                assert_eq!(*game.persist.uuids, uuids_before);

                game.enter_level(Dim::Overworld).unwrap();
                use horizon_client::join_seq::Joined;
                let join = game.mobs[0].join_seq();
                let taken = game.mobs.extract(join).unwrap();
                game.native_level_tick_pre(home);
                game.mobs.put_back(0, taken);
                game.native_level_tick_pre(home);
                game.mobs.retain(|mob| mob.id != 91);
                let uuids_before = game.persist.uuids.clone();
                game.native_level_tick_pre(home);
                assert_eq!(*game.persist.uuids, uuids_before);

                assert_eq!(
                    *heard.lock().unwrap(),
                    [
                        Some((91, [3.5, 70.0, 5.5])),
                        None,
                        None,
                        Some((91, [3.5, 70.0, 5.5])),
                        None,
                    ]
                );
            });
        });
    }

    struct Moments(Arc<Mutex<Vec<(&'static str, LevelRef, Option<i64>)>>>);
    #[mod_api::__subscriptions]
    impl Hooks for Moments {
        fn level_tick_pre(&self, host: HostSide, event: &mut LevelTickPreEvent) {
            let time = match host {
                HostSide::Client(h) => event.level.game_time(h),
                HostSide::Server(h) => event.level.game_time(h),
            };
            self.0.lock().unwrap().push(("pre", event.level, time));
        }
        fn level_tick_post(&self, host: HostSide, event: &mut LevelTickPostEvent) {
            let time = match host {
                HostSide::Client(h) => event.level.game_time(h),
                HostSide::Server(h) => event.level.game_time(h),
            };
            self.0.lock().unwrap().push(("post", event.level, time));
        }
    }

}
