use crate::game::commands::Target;
use crate::game::{entity_residency::SlotAccess, native_guest_turn_tests::with_guests};
use horizon_client::join_seq::residency::table::EntityLookup;
use horizon_client::join_seq::residency::{Kind, Source};
use horizon_core::geometry::Vec3;
use std::{
    num::NonZeroU32,
    sync::{Arc, Mutex},
};

#[test]
#[ignore = "daily"]
fn actual_guest_membership_and_lifetime_survive_nested_body_loans_and_reset() {
    with_guests(|game| {
        use horizon_mod_runtime::attachments::{AttachmentType, Holder, index::AttachmentIndex};
        const VALUE: AttachmentType<i32> = AttachmentType {
            namespace: "source_test",
            name: "value",
            default: || 0,
            codec: None,
            copy_on_death: false,
            sync: false,
        };
        let index = Arc::new(Mutex::new(AttachmentIndex::default()));
        let dimension = game.dim.key();
        let id = NonZeroU32::new(12).unwrap();
        let host = game.lan.as_mut().unwrap();
        host.hub.guests.update(0, |guest| guest.placed = false);
        let attachments = host.hub.guests.attachments_mut(12).unwrap();
        VALUE.set(attachments, Holder::Level, 7).unwrap();
        let registration = attachments.bind_index(&index, 12u32);
        assert!(matches!(
            game.server_resident_entity(12),
            EntityLookup::Found {
                value: Target::Guest(12),
                ..
            }
        ));
        assert_eq!(game.uuid_for(12), [0, 0, 0, 12]);
        assert_eq!(
            game.with_resident_slot(&dimension, id, |slot| slot
                .registration()
                .unwrap()
                .same_registration(&registration)),
            SlotAccess::Done(true)
        );
        game.lan
            .as_mut()
            .unwrap()
            .hub
            .guests
            .update(0, |guest| guest.placed = true);
        game.as_guest(12, |outer| {
            assert!(matches!(
                outer.server_resident_entity(12),
                EntityLookup::Found {
                    value: Target::Player,
                    ..
                }
            ));
            assert_eq!(
                outer.with_resident_slot(&dimension, id, |slot| slot
                    .registration()
                    .unwrap()
                    .same_registration(&registration)),
                SlotAccess::Done(true)
            );
            outer
                .as_guest(13, |inner| {
                    assert_eq!(inner.server_resident_entity(12), EntityLookup::Taken);
                    assert_eq!(
                        inner.with_resident_slot::<()>(&dimension, id, |_| panic!(
                            "displaced body lent a slot"
                        )),
                        SlotAccess::Taken
                    );
                    inner.reset_entity_residency();
                    assert_eq!(inner.server_resident_entity(12), EntityLookup::Taken);
                    assert_eq!(inner.uuid_for(12), [0, 0, 0, 12]);
                    assert!(matches!(
                        inner.server_resident_entity(13),
                        EntityLookup::Found {
                            value: Target::Player,
                            ..
                        }
                    ));
                })
                .unwrap();
            assert_eq!(
                outer.with_resident_slot(&dimension, id, |slot| slot
                    .registration()
                    .unwrap()
                    .same_registration(&registration)),
                SlotAccess::Done(true)
            );
        })
        .unwrap();
        assert_eq!(
            game.with_resident_slot(&dimension, id, |slot| slot
                .registration()
                .unwrap()
                .same_registration(&registration)),
            SlotAccess::Done(true)
        );
        let removed = game.lan.as_mut().unwrap().hub.guests.remove(0);
        assert_eq!(game.server_resident_entity(12), EntityLookup::Missing);
        assert_eq!(
            index
                .lock()
                .unwrap()
                .snapshot(&("source_test".into(), "value".into()))
                .len(),
            0
        );
        drop(removed);
    });
}

#[test]
#[ignore = "daily"]
fn refused_body_construction_does_not_begin_guest_loans() {
    with_guests(|game| {
        assert!(matches!(
            game.server_resident_entity(12),
            EntityLookup::Found {
                value: Target::Guest(12),
                ..
            }
        ));
        let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
        let mut bad =
            horizon_client::motion::MotionResources::load(&root, &game.pack.assets).unwrap();
        bad.attributes = std::sync::Arc::new(horizon_world::attributes::entity::Catalog::from_json(&serde_json::json!({
            "data_version": horizon_core::version::DATA_VERSION, "attributes": [], "entities": {}
        })).unwrap());
        assert!(horizon_client::motion::PlayerMotion::new(&bad, Vec3::ZERO).is_err());
        let original = game.motion;
        game.motion = Box::leak(Box::new(bad));
        game.lan
            .as_mut()
            .unwrap()
            .hub
            .guests
            .update_connection(12, |g| g.motion = None);
        let permission = game.session.registries.permission;
        assert!(game.as_guest(12, |_| panic!("refused turn ran")).is_none());
        game.motion = original;
        assert_eq!(game.session.registries.permission, permission);
        assert!(game.lan_guest_swaps.is_empty());
        assert!(matches!(
            game.server_resident_entity(12),
            EntityLookup::Found {
                value: Target::Guest(12),
                ..
            }
        ));
        game.as_guest(12, |outer| {
            outer
                .as_guest(13, |inner| assert!(inner.as_guest(12, |_| ()).is_none()))
                .unwrap();
        })
        .expect("same connection can enter after refusal");
        assert!(game.lan_guest_swaps.is_empty());
    });
}

#[test]
#[ignore = "daily"]
fn parked_mob_attachment_values_survive_other_level_cleanup() {
    const VALUE: mod_api::AttachmentType<i32> = mod_api::AttachmentType {
        namespace: "source_test",
        name: "parked_cleanup",
        default: || 0,
        codec: None,
        copy_on_death: false,
        sync: false,
    };
    crate::game::native_entity_refs::tests::with_game(|game| {
        game.native_level_opened(game.dim);
        let home = game.dim;
        let id = game
            .spawn_mob_spawned(
                "cow",
                [1.0, 70.0, 3.0],
                crate::game::creatures::SpawnSite::Plain,
            )
            .unwrap();
        VALUE.set(game, mod_api::Holder::Entity(id), 73).unwrap();
        game.enter_level(crate::game::Dim::Nether).unwrap();
        game.prune_entity_attachments();
        game.prune_entity_memory();
        assert!(game.entity_attachments.contains_key(&id));
        game.enter_level(home).unwrap();
        assert_eq!(
            VALUE.get_existing(game, mod_api::Holder::Entity(id)),
            Ok(Some(73))
        );
    });
}
