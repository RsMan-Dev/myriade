//! Owner launch boundary regression with vanilla's deterministic sequence.
#![cfg(test)]
use horizon_client::items::ItemStack;
use horizon_core::random::RandomSource;
use mod_api::Identifier;

#[test]
#[ignore = "daily"]
fn owner_launch_reports_explicit_guest_to_launch_listeners() {
    // Opt in only in a fresh child; never mutate the parallel parent process's environment.
    if crate::game::native_entity_refs::tests::run_fresh_cases(
        "game::native_projectile_launch_tests::owner_launch_reports_explicit_guest_to_launch_listeners",
        "HORIZON_TEST_SEEDS",
        &["1"],
    ) {
        return;
    }
    use mod_api::{Hooks, ModEntry, ProjectileLaunchEvent, ServerHost};
    use std::sync::{Arc, Mutex};
    struct Observe(Arc<Mutex<Vec<(Option<u128>, String, Option<u128>)>>>);
    #[mod_api::__subscriptions]
    impl Hooks for Observe {
        fn projectile_launch(
            &self,
            host: &mut dyn ServerHost,
            event: &mut ProjectileLaunchEvent<'_>,
        ) {
            let projectile = host.projectile(host.entity(event.entity).unwrap()).unwrap();
            let owner = projectile.owner(host).unwrap();
            let uuid = owner.uuid(host);
            self.0
                .lock()
                .unwrap()
                .push((event.player, event.name.to_owned(), uuid));
        }
    }
    let observed = Arc::new(Mutex::new(Vec::new()));
    let mods = vec![ModEntry::new(
        "observe",
        Box::new(Observe(observed.clone())),
    )];
    mod_platform::testing::with_mods(mods.into(), || {
        crate::game::native_guest_turn_tests::with_guests(|game| {
            let source = game.entity(12).unwrap();
            let owner = game.living(source).unwrap();
            let previous_actor = game.lan_acting;
            let stack = ItemStack::new("minecraft:snowball", 1);
            let kind = Identifier::parse("minecraft:snowball").unwrap();
            let shot = game
                .spawn_projectile_from_rotation(owner, &kind, &stack, 0.0, 1.5, 1.0)
                .unwrap();
            assert_eq!(game.projectile_owner(shot), Some(source));
            assert_eq!(game.lan_acting, previous_actor);
            assert_eq!(
                *observed.lock().unwrap(),
                [(Some(12), "guest12".to_owned(), Some(12))]
            );
            game.lan
                .as_mut()
                .unwrap()
                .hub
                .update_guest(12, |guest| {
                    guest.dimension = "minecraft:the_nether".to_owned();
                })
                .unwrap();
            let count = game.ecs.entity_count();
            let mut before = game.world.random.clone();
            horizon_client::random_source::set_thread_uniquifier(2709830126447559882).unwrap();
            assert!(
                game.spawn_projectile_from_rotation(owner, &kind, &stack, 0.0, 1.5, 1.0)
                    .is_none()
            );
            assert_eq!(game.ecs.entity_count(), count);
            assert_eq!(game.world.random.next_long(), before.next_long());
            assert_eq!(
                horizon_client::random_source::create().next_double(),
                0.6465821602909256
            );
            assert_eq!(observed.lock().unwrap().len(), 1);
        });
    });
}
