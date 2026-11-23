//! `MinecartTNT`'s click and fuse: vanilla has no `interact` override, so
//! flint and steel leaves the cart alone (`Entity.interact` passes), and
//! `primeFuse` plays `entity.tnt.primed` on the blocks source only when the
//! cart is not silent.
use super::super::*;
use super::CartKind;
use crate::game::game_tests::tests::server_game;
use myriade_engine::items::ItemStack;

/// A server's game on the prepared resources, or None when they are absent
/// (CI has none: the tests then skip quietly).
fn server(resources: &mut Option<std::sync::Arc<crate::game::Resources>>) -> Option<Game<'_>> {
    std::env::var_os("HORIZON_TEST_RESOURCES")?;
    Some(server_game(resources))
}

/// A lone TNT cart at (0.5, 64, 0.5), spawned through the bridge.
fn tnt_cart(game: &mut Game<'_>, id: u32) {
    // A cart far from every player sleeps: the player's place is told to the level first.
    game.sync_player_facts().unwrap();
    game.ecs_with(|ecs, mw| {
        ecs.spawn_cart_id(mw, CartKind::Tnt.id(), [0.5, 64.0, 0.5], 0.0, &mut || id)
    })
    .unwrap();
}

#[test]
fn flint_and_steel_passes_on_a_tnt_cart_without_lighting_it() {
    let mut resources = None;
    let Some(mut game) = server(&mut resources) else {
        return;
    };
    tnt_cart(&mut game, 42);
    game.inventory[game.selected] = Some(ItemStack::new("minecraft:flint_and_steel", 1));
    game.audio.capture = Some(Vec::new());
    let used = game.interact_cart(Some(InteractTarget {
        id: 42,
        location: [0.0, 0.0, 0.0],
    }));
    assert!(!used, "Entity.interact passes to the held item");
    assert_eq!(game.ecs.cart_fuse(42), Some(-1), "flint and steel lit the cart");
    let sounds = game.audio.capture.take().unwrap();
    assert!(sounds.is_empty(), "{sounds:?}");
}
