//! The adventure-mode gates on a right click: `TntBlock.prime` refuses to
//! light TNT for a player whose held stack's `can_break` does not name it,
//! and `ItemStack.useOn`'s `can_place_on` is what lets a placement through.
use crate::game::{Hit, game_tests::tests::server_game};
use horizon_client::command::args::GameMode;
use horizon_client::items::ItemStack;
use horizon_client::mobs::Who;
use horizon_core::nbt::Tag;

type Posted = (&'static str, [f64; 3], Option<Who>);

/// A server's game with nobody playing, or `None` without prepared resources.
fn game() -> Option<crate::game::Game<'static>> {
    std::env::var_os("HORIZON_TEST_RESOURCES")?;
    let resources = Box::leak(Box::new(None));
    let mut game = server_game(resources);
    game.build_registries();
    game.set_allow_commands(true);
    game.player = None;
    Some(game)
}

/// `stack` with a `can_break` or `can_place_on` component naming `block`.
fn stack_with(name: &str, component: &str, block: &str) -> ItemStack {
    let mut stack = ItemStack::new(name, 1);
    stack.data_mut().other.push((
        component.into(),
        Tag::compound([("blocks", Tag::List(vec![Tag::String(block.into())]))]),
    ));
    stack
}

/// The primed TNT the level holds (the ECS owns them).
fn tnt_count(game: &crate::game::Game<'_>) -> usize {
    game.ecs.count_by_kind().iter().filter(|(k, _)| *k == "minecraft:tnt").map(|(_, n)| n).sum()
}

/// A click on the block at `pos` from above, at the centre of its top face.
fn hit(pos: [i32; 3]) -> Hit {
    Hit {
        pos,
        face: [0, 1, 0],
        point: [
            f64::from(pos[0]) + 0.5,
            f64::from(pos[1]) + 1.0,
            f64::from(pos[2]) + 0.5,
        ],
        inside: false,
    }
}

/// An adventure player right-clicks the block at `pos` with what is in hand,
/// as `Minecraft.startUseItem` does; whether the click consumed the action and
/// the game events the click posted.
fn click(game: &mut crate::game::Game<'_>, pos: [i32; 3]) -> (bool, Vec<Posted>) {
    let hit = hit(pos);
    game.forced_hit = Some(hit);
    crate::game::sculk::TEST_POSTED_EVENTS.with(|events| *events.borrow_mut() = Some(Vec::new()));
    let consumed = game.with_player_actor(|g| g.use_hand(Some(&hit))).unwrap();
    // The ECS hands the fuse event over at its next tick (vanilla posts it in the same one).
    game.tick_ecs().unwrap();
    game.forced_hit = None;
    let events =
        crate::game::sculk::TEST_POSTED_EVENTS.with(|events| events.borrow_mut().take().unwrap());
    (consumed, events)
}

/// `TntBlock.prime`: an adventure player lights TNT only with a stack whose
/// `can_break` names it. A refused click is still `TntBlock.useItemOn`'s
/// SUCCESS, so nothing but the swing happens: no fuse, no sound, no wear.
#[test]
fn an_adventure_click_primes_tnt_only_with_a_can_break_that_names_it() {
    let Some(mut game) = game() else { return };
    assert!(
        game.session
            .rules
            .bool_at(horizon_client::gamerules::vanilla::TNT_EXPLODES)
    );
    let pos = [2, 4, 2];
    let tnt = game
        .pack
        .assets
        .registry
        .default_state("minecraft:tnt")
        .unwrap();
    game.world.set(pos[0], pos[1], pos[2], tnt).unwrap();
    game.set_mode(GameMode::Adventure);
    let air = game.res.facts().air;

    // No `can_break`: the block stays, the fuse is not lit and the flint and
    // steel is not worn.
    *game.held_mut() = Some(ItemStack::new("minecraft:flint_and_steel", 1));
    let (consumed, events) = click(&mut game, pos);
    assert!(
        consumed,
        "TntBlock.useItemOn's SUCCESS still ends the click"
    );
    assert!(
        tnt_count(&game) == 0,
        "an adventure player without can_break primes nothing"
    );
    assert_eq!(game.world.block(pos[0], pos[1], pos[2]), tnt);
    assert_eq!(game.held().map(|s| s.damage), Some(0));
    assert!(events.is_empty(), "no game event, got {events:?}");

    // A `can_break` naming TNT lets the same click prime it: the block goes,
    // the fuse event names the player, and the flint and steel wears by one.
    *game.held_mut() = Some(stack_with(
        "minecraft:flint_and_steel",
        "minecraft:can_break",
        "minecraft:tnt",
    ));
    let events = click(&mut game, pos).1;
    assert_eq!(tnt_count(&game), 1);
    assert_eq!(game.world.block(pos[0], pos[1], pos[2]), air);
    assert_eq!(game.held().map(|s| s.damage), Some(1));
    assert_eq!(
        events,
        vec![("minecraft:prime_fuse", [2.5, 4.5, 2.5], Some(Who::Player))]
    );
}
