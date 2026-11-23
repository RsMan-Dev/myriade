//! `EnderDragonFight.updateDragon`: the fight's boss bar takes the dragon's
//! custom name (`dragon.getDisplayName()`), and keeps the type's name until
//! it has one.
use super::*;
use myriade_engine::chunk::Chunk;

fn end_game() -> Game<'static> {
    let root = std::path::PathBuf::from(
        std::env::var_os("HORIZON_TEST_RESOURCES")
            .expect("set HORIZON_TEST_RESOURCES to prepared resources"),
    );
    let settings = Settings::default();
    let loaded = load_resources(
        horizon_assets::resource_source::ResourceRoot::directory(root.clone()),
        Vec::new(),
        None,
        &root,
        &settings,
        std::time::Instant::now(),
        crate::game::execution_role::ExecutionRole::DedicatedServer,
        &Progress::new(),
    )
    .unwrap();
    let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
    options.resources = root.clone();
    options.execution_role = execution_role::ExecutionRole::DedicatedServer;
    let res: &'static _ = Box::leak(Box::new(loaded.resources));
    let mut game = Game::new(
        res,
        loaded.pack,
        options,
        settings,
    )
    .unwrap();
    game.build_registries();
    game.set_allow_commands(true);
    assert_eq!(game.enter_command_level(Dim::End), Some(Dim::Overworld));
    assert!(game.dimension_type().has_ender_dragon_fight, "the End");
    game
}

#[test]
#[ignore = "daily"]
fn the_dragon_boss_bar_takes_the_dragons_custom_name() {
    let mut game = end_game();
    crate::game::native_guest_turn_tests::lan_guests_on(&mut game);
    // The chunks `EnderDragonFight.isArenaLoaded` waits for, lit as received
    // chunks are, so the fight's next player scan adopts or makes its dragon.
    {
        let streaming = &mut game.streaming;
        let world = &mut game.world;
        for z in -2..=1 {
            for x in -2..=1 {
                streaming.insert_remote(world, Chunk::empty(x, z));
            }
        }
    }
    // `updatePlayers` scans every 20 ticks; then `findOrCreateDragon` makes
    // the fight's dragon.
    for _ in 0..20 {
        game.tick_fight();
    }
    let id = game.dragon_views().into_iter().find(|(_, v)| v.fight).map(|(d, _)| d).expect("the fight's dragon");
    game.publish_boss_bars();
    assert!(
        game.bosses.dragon_name.is_none(),
        "the type's name until it has a custom one"
    );
    // The name a summoned dragon carries: `remember_tags` keeps its
    // `CustomName` for `/data get`, and `updateDragon` reads it.
    game.ecs.set_custom_name(id, Some("Bob".into()));
    game.publish_boss_bars();
    assert!(format!("{:?}", game.bosses.dragon_name).contains("Bob"));
}
