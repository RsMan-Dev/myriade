//! A server `Game` with an active ECS level, for the tests of the ECS's host side. The resources are leaked: a test process keeps them anyway.
use crate::game::{Audio, Game, Options, Progress, Settings, execution_role::ExecutionRole, load_resources};
use std::path::PathBuf;

/// A game on an integrated server's role whose ECS level is active, with one `kind` spawned at `at`: the game and the entity's network id.
pub(in crate::game) fn ecs_game_with(kind: &str, at: [f64; 3]) -> (Game<'static>, u32) {
    let root = PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("set HORIZON_TEST_RESOURCES to prepared resources"));
    let settings = Settings::default();
    let loaded = load_resources(
        horizon_assets::resource_source::ResourceRoot::directory(root.clone()),
        Vec::new(),
        None,
        &root,
        &settings,
        std::time::Instant::now(),
        ExecutionRole::IntegratedServer,
        &Progress::new(),
    )
    .unwrap();
    let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
    options.resources = root.clone();
    options.execution_role = ExecutionRole::IntegratedServer;
    options.game_dir = Some(std::env::temp_dir().join(format!("horizon-ecs-test-{}", std::process::id())));
    let resources = Box::leak(Box::new(loaded.resources));
    let mut game = Game::new(
        resources,
        loaded.pack,
        options,
        settings,
        Audio::default(),
    )
    .unwrap();
    game.reset_entity_residency();
    game.native_lifecycle.begin_open(mod_api::Authority::IntegratedServer);
    game.ecs = game.test_ecs_level();
    let id = game.ecs_spawn(kind, at, None).expect("a known species");
    (game, id)
}

/// The `minecraft:item` compound of a test: a stone at 0.5, 0, 0.5, with `fields` (SNBT pairs) added.
pub(in crate::game) fn item_compound(fields: &str) -> horizon_core::nbt::Compound {
    let snbt = format!("{{id:\"minecraft:item\",Pos:[0d,0d,0d],Item:{{id:\"minecraft:stone\",count:1}},{fields}}}");
    match horizon_core::nbt::parse_snbt(&snbt).unwrap().0 {
        horizon_core::nbt::Tag::Compound(c) => c,
        _ => unreachable!("a compound literal"),
    }
}

/// Makes the entity of `c` in the test game's ECS (`load_entity`) and gives back what it saves; None when it is not made.
pub(in crate::game) fn ecs_roundtrip(game: &mut Game<'_>, c: &horizon_core::nbt::Compound) -> Option<horizon_core::nbt::Compound> {
    let e = game.ecs_with(|ecs, mw| ecs.load_entity(mw, c))?;
    game.ecs.entity_nbt(e)
}

/// Hangs a frame (glow or not) on `face` of `at` in the test game's ECS, placed by nobody; its id, None when the wall does not hold.
pub(in crate::game) fn hang_test_frame(game: &mut Game<'_>, at: [i32; 3], face: u8, glow: bool) -> Option<u32> {
    let mut next = game.next_entity;
    let id = game.ecs_with(|ecs, mw| {
        ecs.hang_frame(mw, at, face, glow, None, &mut || {
            next += 1;
            next
        })
    });
    game.next_entity = next;
    id
}

/// Hangs the painting `variant` on `face` of `at` in the test game's ECS; its id, None when the variant is unknown or the wall does not hold.
pub(in crate::game) fn hang_test_painting(game: &mut Game<'_>, at: [i32; 3], face: u8, variant: &str) -> Option<u32> {
    let mut next = game.next_entity;
    let id = game.ecs_with(|ecs, mw| {
        ecs.hang_painting(mw, at, face, Some(variant), None, &mut |_| 0, &mut || {
            next += 1;
            next
        })
    });
    game.next_entity = next;
    id
}

/// A test game with its ECS level active (`test_ecs_level`), over the shared test server.
pub(in crate::game) fn active_server_game(resources: &mut Option<std::sync::Arc<crate::game::Resources>>) -> Game<'_> {
    let mut game = crate::game::game_tests::tests::server_game(resources);
    game.ecs = game.test_ecs_level();
    game
}
