use super::*;
use crate::game::{
    Dim,
    native_entity_refs::tests::with_game,
};
use horizon_core::nbt::Tag;
use mod_api::{AttachmentType, EntityConstructingEvent, Hooks, HostSide};
use std::sync::{Arc, Mutex};

const VALUE: AttachmentType<i32> = AttachmentType {
    namespace: "constructing",
    name: "value",
    default: || 0,
    codec: None,
    copy_on_death: false,
    sync: false,
};
type Calls = Arc<Mutex<Vec<(bool, Holder)>>>;

struct Listener(Calls);

#[mod_api::__subscriptions]
impl Hooks for Listener {
    fn entity_constructing(&self, host: HostSide<'_>, event: &mut EntityConstructingEvent) {
        self.0
            .lock()
            .unwrap()
            .push((matches!(host, HostSide::Client(_)), event.holder));
        // Both a read and a write inside dispatch must reach the canonical map
        // without recursively constructing the same entity.
        match host {
            HostSide::Server(host) => {
                let _ = VALUE.get(host, event.holder).unwrap();
                VALUE.set(host, event.holder, 41).unwrap();
            }
            HostSide::Client(host) => {
                let _ = VALUE.get(host, event.holder).unwrap();
                VALUE.set(host, event.holder, 41).unwrap();
            }
        }
    }
}

fn observe(test: impl FnOnce(Calls)) {
    let calls = Calls::default();
    let mods = vec![mod_api::ModEntry::new(
        "constructing",
        Box::new(Listener(calls.clone())),
    )]
    .into();
    mod_platform::testing::with_mods(mods, || test(calls));
}

fn tag(kind: &str, uuid: i32) -> Tag {
    Tag::compound([
        ("id", Tag::String(kind.into())),
        ("UUID", Tag::IntArray(vec![uuid, 2, 3, 4])),
        (
            "Pos",
            Tag::List(
                [1.0f64, 70.0, 3.0]
                    .map(|v| Tag::Double(v.to_bits()))
                    .to_vec(),
            ),
        ),
    ])
}

#[test]
#[ignore = "daily"]
fn spawn_summon_and_disk_load_initialize_before_first_attachment_read() {
    observe(|calls| {
        with_game(|game| {
            game.options.dedicated = Some(Vec::new());
            let spawn = game
                .spawn_mob_spawned(
                    "cow",
                    [1.0, 70.0, 3.0],
                    crate::game::creatures::SpawnSite::Plain,
                )
                .unwrap();
            assert_eq!(VALUE.get(game, Holder::Entity(spawn)), Ok(41));
            game.spawn_entity_from_nbt(tag("minecraft:pig", 100));
            let summoned = game
                .persist
                .uuids
                .iter()
                .find(|(_, uuid)| **uuid == [100, 2, 3, 4])
                .map(|(id, _)| *id)
                .unwrap();
            assert_eq!(VALUE.get(game, Holder::Entity(summoned)), Ok(41));
            for (i, kind) in [
                "minecraft:zombie",
                "minecraft:arrow",
                "minecraft:snowball",
                "minecraft:oak_boat",
                "minecraft:minecart",
                "minecraft:armor_stand",
                "minecraft:marker",
            ]
            .into_iter()
            .enumerate()
            {
                let uuid = 101 + i as i32;
                game.world
                    .loaded_entities
                    .push(((0, 0), vec![tag(kind, uuid).into()]));
                game.spawn_loaded_entities();
                let id = *game
                    .persist
                    .uuids
                    .iter()
                    .find(|(_, value)| **value == [uuid, 2, 3, 4])
                    .unwrap()
                    .0;
                let expected = Ok(41);
                assert_eq!(VALUE.get(game, Holder::Entity(id)), expected, "{kind}");
                assert_eq!(
                    calls
                        .lock()
                        .unwrap()
                        .iter()
                        .filter(|(_, h)| *h == Holder::Entity(id))
                        .count(),
                    1,
                    "{kind}"
                );
            }
            let count = calls.lock().unwrap().len();
            game.native_joins_flush(false, 0);
            game.native_joins_flush(true, 0);
            game.entity_chunks(&HashSet::from([(0, 0)]), false);
            game.remember_uuid(summoned, Some([100, 2, 3, 4]), &tag("minecraft:pig", 100));
            assert_eq!(calls.lock().unwrap().len(), count);
            assert!(calls.lock().unwrap().iter().all(|(client, _)| !client));
        })
    });
}

fn spawn_info() -> horizon_protocol::packets::play::SpawnInfo {
    horizon_protocol::packets::play::SpawnInfo {
        dimension_type: 0,
        dimension: "minecraft:overworld".into(),
        seed: 1,
        game_type: 0,
        previous_game_type: None,
        debug: false,
        flat: false,
        last_death: None,
        portal_cooldown: 0,
        sea_level: 63,
    }
}

#[test]
#[ignore = "daily"]
fn server_players_initialize_on_join_and_new_respawn_but_not_rebind() {
    observe(|calls| {
        crate::game::native_guest_turn_tests::with_guests(|game| {
            game.options.dedicated = Some(Vec::new());
            game.world_root = game.save.root.clone();
            game.lan
                .as_mut()
                .unwrap()
                .hub
                .guests
                .update_all(|guest| guest.placed = false);
            game.lan_guest_joined(12, &[], [0.5, 64.0, 0.5]);
            assert_eq!(VALUE.get(game, Holder::Player(12)), Ok(41));
            game.bind_player_attachments(12, false);
            game.initialize_server_player(12, 12);
            assert_eq!(
                calls.lock().unwrap().as_slice(),
                &[(false, Holder::Player(12))]
            );
            game.lan
                .as_mut()
                .unwrap()
                .hub
                .guests
                .update(0, |guest| guest.dead = true);
            game.guest_respawn(12);
            assert_eq!(VALUE.get(game, Holder::Player(12)), Ok(41));
            assert_eq!(
                calls.lock().unwrap().as_slice(),
                &[(false, Holder::Player(12)), (false, Holder::Player(12))]
            );
        })
    });
}

#[test]
#[ignore = "daily"]
fn marker_constructor_store_reaches_its_real_owned_map() {
    use std::sync::atomic::{AtomicBool, Ordering};
    struct Store(Arc<AtomicBool>);
    #[mod_api::__subscriptions]
    impl Hooks for Store {
        fn entity_constructing(&self, host: HostSide<'_>, event: &mut EntityConstructingEvent) {
            if let HostSide::Server(host) = host {
                assert_eq!(VALUE.set(host, event.holder, 41), Ok(()));
                self.0.store(true, Ordering::SeqCst);
            }
        }
    }
    let ran = Arc::new(AtomicBool::new(false));
    mod_platform::testing::with_mods(
        vec![mod_api::ModEntry::new(
            "store",
            Box::new(Store(ran.clone())),
        )]
        .into(),
        || {
            with_game(|game| {
                game.options.dedicated = Some(Vec::new());
                game.native_level_opened(Dim::Overworld);
                game.spawn_entity_from_nbt(tag("minecraft:marker", 909));
            });
        },
    );
    assert!(ran.load(Ordering::SeqCst));
}

#[test]
#[ignore = "daily"]
fn cancelled_mob_creation_forgets_constructor_payload_and_identity() {
    struct Cancel;
    #[mod_api::__subscriptions]
    impl Hooks for Cancel {
        fn finalize_spawn(
            &self,
            _: &dyn mod_api::Host,
            event: &mut mod_api::FinalizeSpawnEvent<'_>,
        ) -> mod_api::Flow {
            event.spawn_cancelled = true;
            mod_api::Flow::Continue
        }
    }
    let calls = Calls::default();
    mod_platform::testing::with_mods(
        vec![
            mod_api::ModEntry::new("constructing", Box::new(Listener(calls.clone()))),
            mod_api::ModEntry::new("cancel", Box::new(Cancel)),
        ]
        .into(),
        || {
            with_game(|game| {
                game.options.dedicated = Some(Vec::new());
                game.native_level_opened(Dim::Overworld);
                let before = game.persist.uuids.clone();
                assert!(
                    game.spawn_mob_spawned(
                        "cow",
                        [1.0, 70.0, 3.0],
                        crate::game::creatures::SpawnSite::Finalizing(
                            crate::game::creatures::SpawnReason::Command
                        )
                    )
                    .is_none()
                );
                assert_eq!(calls.lock().unwrap().len(), 1);
                assert_eq!(*game.persist.uuids, before);
                assert!(game.entity_attachments.is_empty());
                assert!(game.entity_initialization.initialized.is_empty());
            });
        },
    );
}

