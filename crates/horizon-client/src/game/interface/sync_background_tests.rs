//! Regressions for the typed bindings carried by the upstream sync.
use super::native_entity_refs::tests::with_game_at;
use horizon_client::chunk::Chunk;
use mod_api::{Flow, Hooks, ModEntry, ServerHost};
use std::{path::PathBuf, sync::Arc};

mod claims {
    use super::*;
    #[derive(Default)]
    struct Claims;
    #[mod_api::horizon_mod]
    impl Hooks for Claims {
        fn entity_change_block_pre(
            &self,
            host: &mut dyn ServerHost,
            event: &mut mod_api::EntityChangeBlockPreEvent<'_>,
        ) -> Flow {
            assert_eq!(event.player, Some(7));
            assert_eq!(event.entity, None);
            assert_eq!(event.kind, "minecraft:player");
            assert_eq!(Some(event.from), host.block_state_id("minecraft:farmland"));
            assert_eq!(Some(event.to), host.block_state_id("minecraft:dirt"));
            Flow::Cancel
        }
    }
}
#[test]
#[ignore = "daily"]
fn cancelled_landing_restores_farmland_before_its_crop_is_notified() {
    let root = PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
    mod_platform::testing::with_mods(
        vec![ModEntry::new("claims", claims::hooks())].into(),
        || {
            with_game_at(root, |game| {
                game.world
                    .chunks
                    .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
                let state = |name| game.pack.assets.registry.default_state(name).unwrap();
                let (farmland, dirt, wheat) = (
                    state("minecraft:farmland"),
                    state("minecraft:dirt"),
                    state("minecraft:wheat"),
                );
                game.world.set(1, 64, 1, dirt).unwrap();
                game.world.set(1, 65, 1, wheat).unwrap();
                game.landing_block_updates(
                    Some(7),
                    &horizon_client::landing::FallEvent::Write {
                        position: horizon_core::position::BlockPos::new(1, 64, 1),
                        from: farmland,
                        state: dirt,
                        flags: 3,
                    },
                );
                assert_eq!(game.world.block(1, 64, 1), farmland);
                assert_eq!(game.world.block(1, 65, 1), wheat);
            })
        },
    );
}
