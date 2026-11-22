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

/// A test game with its ECS level active (`test_ecs_level`), over the shared test server.
pub(in crate::game) fn active_server_game(resources: &mut Option<std::sync::Arc<crate::game::Resources>>) -> Game<'_> {
    let mut game = crate::game::game_tests::tests::server_game(resources);
    game.ecs = game.test_ecs_level();
    game
}
