use crate::game::{Audio, Game, Options, Progress, Settings, load_resources};
use horizon_client::{chunk::Chunk, motion::PlayerMotion};
use horizon_core::{geometry::Vec3, position::ChunkPos, random::RandomSource};
use horizon_ecs_proto::bridge::select::Select;
use std::sync::Arc;
fn with_game(test: impl FnOnce(&mut Game<'_>)) {
    let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
    let settings = Settings::default();
    let loaded = load_resources(
        horizon_assets::resource_source::ResourceRoot::directory(root.clone()),
        Vec::new(),
        None,
        &root,
        &settings,
        std::time::Instant::now(),
        crate::game::execution_role::ExecutionRole::IntegratedServer,
        &Progress::new(),
    )
    .unwrap();
    let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
    options.resources = root.clone();
    options.game_dir = Some(
        std::path::PathBuf::from(std::env::var_os("HORIZON_HOME").unwrap())
            .join(format!("h114-{:?}", std::thread::current().id())),
    );
    options.execution_role = crate::game::execution_role::ExecutionRole::IntegratedServer;
    let mut game = Game::new(
        &loaded.resources,
        loaded.pack,
        options,
        settings,
        Audio::default(),
    )
    .unwrap();
    game.build_registries();
    game.set_allow_commands(true);

    game.world
        .chunks
        .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
    game.block_ticks
        .add_empty_chunk(ChunkPos::new(0, 0))
        .unwrap();
    assert!(game.run_command("difficulty normal"));
    assert!(game.run_command("forceload add 0 0"));
    for (x, z) in [(0, 1), (1, 0), (1, 1)] {
        game.world
            .chunks
            .insert((x, z), Arc::new(Chunk::empty(x, z)));
        game.block_ticks
            .add_empty_chunk(ChunkPos::new(x, z))
            .unwrap();
    }
    game.player = Some(PlayerMotion::new(game.motion, Vec3::new(2.5, 200.0, 2.5)).unwrap());
    game.creative = true;
    assert!(game.run_command("fill 0 199 0 31 199 31 stone"));
    assert!(
        game.run_command("summon pig 13.5 200 2.5 {UUID:[I;114,0,0,99],NoAI:1b,NoGravity:1b}")
    );
    game.world.random.set_seed(114);
    test(&mut game);
}
/// `ItemEntity.tick`: an item that drops through water onto honey in one tick
/// slides (`HoneyBlock.fallOn`, during `Entity.move`) before it splashes
/// (`updateFluidInteraction`'s water entry, after the move).
#[test]
#[ignore = "daily"]
fn an_item_dropping_through_water_onto_honey_slides_before_it_splashes() {
    with_game(|game| {
        assert!(game.run_command("setblock 6 200 10 honey_block"));
        assert!(game.run_command("setblock 6 201 10 water"));
        game.dropped.clear();
        game.spawn_item(
            crate::items::ItemStack::new("minecraft:stone", 1),
            [6.5, 202.05, 10.5],
            [0.0, -1.2, 0.0],
            10,
        );
        game.dropped[0].ticks = 1;
        game.ticked_entities_through = u32::MAX;
        game.audio.capture = Some(Vec::new());
        game.tick_items();
        let sounds = game.audio.capture.take().unwrap();
        let order: Vec<&str> = sounds
            .iter()
            .map(|s| &*s.event)
            .filter(|e| *e == "minecraft:block.honey_block.slide" || e.ends_with(".splash"))
            .collect();
        assert_eq!(
            order,
            [
                "minecraft:block.honey_block.slide",
                "minecraft:entity.generic.splash"
            ],
            "{sounds:?} at {:?}",
            game.dropped.first().map(|i| i.position)
        );
    });
}

#[test]
#[ignore = "daily"]
fn guest_crossbow_shot_leaves_the_host_context_as_it_was() {
    use horizon_client::lan::{Joined, NetConfig, PlayerSender, guest::Guest, hub::HubConfig};
    use horizon_protocol::packets::configuration::ClientInformation;
    use std::sync::{atomic::AtomicI32, mpsc};
    with_game(|game| {
        game.lan_host(
            crate::game::lan::LanSettings {
                port: 0,
                guest_commands: true,
                force_game_mode: false,
            },
            NetConfig {
                announce: false,
                ..NetConfig::loopback(0, "arrow-test".into(), 2)
            },
            HubConfig {
                default_game_mode: 1,
                guest_commands: true,
                force_game_mode: false,
                hardcore: false,
                view_distance: 2,
                simulation_distance: 2,
                dimension: "minecraft:overworld".into(),
                allow_flight: false,
                seed: 0,
                max_players: 2,
                integrated: false,
                levels: Default::default(),
                level_seeds: Default::default(),
            },
        )
        .unwrap();
        let (tx, _rx) = mpsc::channel();
        let mut guest = Guest::new(
            Joined {
                conn: 2,
                uuid: 114,
                name: "ArrowGuest".into(),
                properties: Vec::new(),
                info: ClientInformation::new(2),
                brand: "test".into(),
                sender: PlayerSender::new(tx, Arc::default()),
                latency: Arc::new(AtomicI32::new(0)),
                address: "127.0.0.1:25565".parse().unwrap(),
                expected: false,
            },
            2,
            [6.5, 200.0, 6.5],
            2,
        );
        guest.dimension = "minecraft:overworld".into();
        guest.placed = true;
        guest.loaded = true;
        guest.permission = 4;
        game.lan.as_mut().unwrap().hub.guests.push(guest);
        let host_uuid = game.acting_player_uuid();
        game.as_guest(2,|g| {
            assert_eq!(g.lan_acting,Some(2));
            assert_eq!(g.acting_player_uuid(),[0,0,0,114]);
            assert!(g.run_command("item replace entity @s weapon.mainhand with crossbow[charged_projectiles=[{id:\"minecraft:arrow\",count:1}]]"));
            let bow=g.held().unwrap().clone();assert!(g.use_crossbow(&bow));
        }).unwrap();
        assert_eq!(game.acting_player_uuid(), host_uuid);
        assert_eq!(game.lan_acting, None);
        assert_eq!(game.lan.as_ref().unwrap().hub.guests[0].uuid, 114);
        let arrows = Select { kind: Some("minecraft:arrow".into()), ..Default::default() };
        assert_eq!(game.ecs.select(&arrows).len(), 1, "the guest's shot is in the level");
    });
}
