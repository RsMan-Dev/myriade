use horizon_core::geometry::Vec3;
use horizon_core::nbt::Tag;

/// The spawner of `snbt`, loaded into the ECS level of `game` (`ecs_load`).
fn load_spawner(game: &mut crate::game::Game<'_>, snbt: &str) {
    let (tag, _) = horizon_core::nbt::parse_snbt(snbt).unwrap();
    let Tag::Compound(c) = tag else { panic!("a compound") };
    game.ecs_load(&c).expect("the spawner loads");
}

#[test]
#[ignore = "daily"]
fn warning_dispatch_uses_neutral_at_tick_four() {
    use crate::game::{Game, Options, Progress, Settings, load_resources};
    use horizon_client::motion::PlayerMotion;
    let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
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
    options.execution_role = crate::game::execution_role::ExecutionRole::DedicatedServer;
    let mut game = Game::new(
        &loaded.resources,
        loaded.pack,
        options,
        settings,
    )
    .unwrap();
    let scratch = std::env::temp_dir().join(format!(
        "horizon-ominous-sound-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&scratch).unwrap();
    game.world_root = scratch.clone();
    game.player = Some(PlayerMotion::new(game.motion, Vec3::new(4.5, 100.0, 4.5)).unwrap());
    game.reset_entity_residency();
    game.native_lifecycle.begin_open(mod_api::Authority::DedicatedServer);
    game.ecs = game.test_ecs_level();

    const WARNING: &str = "minecraft:block.trial_spawner.about_to_spawn_item";
    let center = [6.5, 102.5, 4.5];
    game.audio.capture = Some(Vec::new());
    game.audio
        .play("minecraft:block.stone.break", Some(center), 1.0, 1.0);
    game.audio.play_on(
        6,
        "minecraft:block.note_block.pling",
        Some(center),
        1.0,
        1.0,
    );
    assert_eq!(
        game.audio
            .capture
            .as_ref()
            .unwrap()
            .iter()
            .map(|s| s.source)
            .collect::<Vec<_>>(),
        [4, 6]
    );
    game.audio.capture.as_mut().unwrap().clear();
    load_spawner(&mut game, r#"{id:"minecraft:ominous_item_spawner",UUID:[I;128,0,0,1],Pos:[6.25d,102.75d,4.875d],item:{id:"minecraft:diamond",count:1},spawn_item_after_ticks:40L}"#);
    assert_eq!(game.ominous_ids().len(), 1);
    for tick in 1..=5 {
        game.tick_ecs().unwrap();
        let warning: Vec<_> = game
            .audio
            .capture
            .as_ref()
            .unwrap()
            .iter()
            .filter(|s| s.event == WARNING)
            .collect();
        assert_eq!(warning.len(), if tick >= 4 { 1 } else { 0 });
        if tick == 4 {
            let sound = warning[0];
            assert_eq!(sound.position, center);
            assert_eq!(sound.volume.to_bits(), 1.0f32.to_bits());
            assert_eq!(sound.pitch.to_bits(), 1.0f32.to_bits());
            assert_eq!(
                sound.source, 6,
                "OminousItemSpawner.tickServer explicitly selects NEUTRAL"
            );
        }
    }
    game.audio
        .play("minecraft:block.stone.break", Some(center), 1.0, 1.0);
    assert_eq!(
        game.audio.capture.as_ref().unwrap().last().unwrap().source,
        4,
        "the explicit warning source must not change later event-family inference"
    );
    drop(game);
    std::fs::remove_dir_all(scratch).unwrap();
}

#[test]
#[ignore = "daily"]
fn residency_spawner_load_expiry_and_spawn_order() {
    crate::game::native_entity_refs::tests::with_game(|game| {
        game.ecs_clear_items();
        for (item, delay) in [("diamond", 1), ("emerald", 3), ("gold_ingot", 1)] {
            load_spawner(game, &format!(
                "{{Pos:[0d,80d,0d],item:{{id:\"minecraft:{item}\",count:1}},spawn_item_after_ticks:{delay}L}}"
            ));
        }
        let survivor = game.ominous_ids()[1];
        game.tick_ecs().unwrap();
        assert_eq!(game.ominous_ids(), [survivor]);
        assert_eq!(
            game.ecs.item_views()
                .iter()
                .map(|e| e.stack.item.as_ref())
                .collect::<Vec<_>>(),
            ["minecraft:diamond", "minecraft:gold_ingot"]
        );
        game.tick_ecs().unwrap();
        game.tick_ecs().unwrap();
        assert!(game.ominous_ids().is_empty());
        assert_eq!(
            game.ecs.item_views().pop().unwrap().stack.item.as_ref(),
            "minecraft:emerald"
        );
    });
}
