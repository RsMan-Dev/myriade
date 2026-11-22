//! `ItemStack.useOn`'s adventure-mode gate: every item's own `useOn` runs
//! only where `Player.mayBuild` holds or the stack's `can_place_on` matches
//! the clicked block. Tools (an axe), honeycomb, bone meal, shears, a water
//! bottle and a spawn egg used on a block pass in adventure mode otherwise.
use crate::game::game_tests::tests::server_game;
use crate::game::{Game, Hit};
use horizon_client::command::args::GameMode;
use mod_api::client::first_person::Hand;
use horizon_client::items::ItemStack;
use horizon_core::nbt::Tag;
use horizon_world::registry::BlockStateId;

/// A server's game with the prepared resources, or `None` where CI has none.
fn game() -> Option<Game<'static>> {
    std::env::var_os("HORIZON_TEST_RESOURCES")?;
    let resources = Box::leak(Box::new(None));
    let mut game = server_game(resources);
    game.build_registries();
    Some(game)
}

/// `stack` with a `can_place_on` component naming `block`.
fn placeable(name: &str, count: u32, block: &str) -> ItemStack {
    let mut stack = ItemStack::new(name, count);
    stack.data_mut().other.push((
        "minecraft:can_place_on".into(),
        Tag::compound([("blocks", Tag::List(vec![Tag::String(block.into())]))]),
    ));
    stack
}

/// Puts `name` at `pos` with `properties` overridden, returning its state.
fn place(
    game: &mut Game<'_>,
    pos: [i32; 3],
    name: &str,
    properties: &[(&str, &str)],
) -> BlockStateId {
    let registry = &game.pack.assets.registry;
    let mut state = registry
        .state(registry.default_state(name).unwrap())
        .unwrap()
        .clone();
    for (key, value) in properties {
        state.properties.insert((*key).into(), (*value).into());
    }
    let id = registry.id(&state).unwrap();
    game.world.set(pos[0], pos[1], pos[2], id).unwrap();
    id
}

/// The top face of `pos`, as `Minecraft.hitResult` names it.
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

/// `Minecraft.startUseItem`'s own prediction runs the same gate on the client:
/// in adventure mode off a matching `can_place_on` an axe does not predict the
/// strip (no sound, no swing), while a builder or a matching stack does.
#[test]
fn an_adventure_use_on_is_not_predicted_on_the_client() {
    let Some(mut game) = game() else {
        return;
    };
    game.set_mode(GameMode::Adventure);
    let log = [2, 4, 3];
    place(&mut game, log, "minecraft:oak_log", &[]);
    let axe = ItemStack::new("minecraft:iron_axe", 1);
    *game.held_mut() = Some(axe.clone());
    assert!(
        !predicts(&mut game, &axe, &hit(log)),
        "the axe's strip was predicted"
    );
    let matching = placeable("minecraft:iron_axe", 1, "minecraft:oak_log");
    *game.held_mut() = Some(matching.clone());
    assert!(
        predicts(&mut game, &matching, &hit(log)),
        "a matching can_place_on still predicts the strip"
    );
    game.set_mode(GameMode::Survival);
    assert!(
        predicts(&mut game, &matching, &hit(log)),
        "a builder's axe still predicts the strip"
    );
}

/// The client's `Item.useOn` prediction behind `ItemStack.useOn`'s adventure gate, which the
/// fork's use phase applies before it selects the prediction.
fn predicts(game: &mut Game<'_>, stack: &ItemStack, hit: &Hit) -> bool {
    game.with_hand(Hand::Main, |g| {
        g.may_use_on(hit.pos) && g.predict_item_on_block(stack, hit)
    })
}
