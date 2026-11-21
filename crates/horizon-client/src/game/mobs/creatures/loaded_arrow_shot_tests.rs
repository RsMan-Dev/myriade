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
        false,
        crate::game::execution_role::ExecutionRole::IntegratedServer,
        &Progress::new(),
        &mut |_, _| unreachable!(),
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
        loaded.gui.unwrap(),
        options,
        settings,
        Audio::silent(&horizon_assets::resource_source::ResourceSource::directory(
            root.clone(),
        )),
        None,
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
fn start_events() {
    crate::game::sculk::TEST_POSTED_EVENTS.with(|v| *v.borrow_mut() = Some(Vec::new()));
}
fn stop_events() {
    crate::game::sculk::TEST_POSTED_EVENTS.with(|v| *v.borrow_mut() = None);
}
/// #14440: an orb's move adds its drop to the fall outside water; landing
/// after a fall posts HIT_GROUND and forgets the fall (`Entity.checkFallDamage`);
/// in water the drop adds nothing, so a landing posts nothing.
#[test]
#[ignore = "daily"]
fn a_landing_orb_posts_hit_ground_after_a_fall() {
    use horizon_client::entities::OrbMove;
    with_game(|game| {
        assert!(
            game.run_command("summon experience_orb 6.5 200 10.5 {Value:1,fall_distance:2d}")
        );
        let i = game.orbs.len() - 1;
        let from = Vec3::new(6.5, 200.5, 10.5);
        let down = Vec3::new(0.0, -0.5, 0.0);
        start_events();
        // In the air: the drop adds to the fall.
        game.orbs[i].on_ground = false;
        game.orb_moved(
            i,
            OrbMove {
                from,
                moved: down,
                in_water: false,
            },
        );
        assert_eq!(game.orbs[i].fall_distance, 2.5);
        // On the ground: HIT_GROUND, then the fall is gone.
        game.orbs[i].on_ground = true;
        game.orb_moved(
            i,
            OrbMove {
                from,
                moved: down,
                in_water: false,
            },
        );
        assert_eq!(game.orbs[i].fall_distance, 0.0);
        // In water with no fall: no drop, so no landing event.
        game.orb_moved(
            i,
            OrbMove {
                from,
                moved: down,
                in_water: true,
            },
        );
        let events: Vec<_> = crate::game::sculk::TEST_POSTED_EVENTS.with(|v| {
            std::mem::take(v.borrow_mut().as_mut().unwrap())
                .into_iter()
                .filter(|e| e.0 == "minecraft:hit_ground")
                .collect()
        });
        stop_events();
        assert_eq!(events.len(), 1);
        assert_eq!(game.orbs[i].fall_distance, 0.0);
    });
}
/// `Entity.checkFallDamage`: an orb landing after a fall runs the block's
/// `fallOn` before HIT_GROUND: honey slides (its sound), farmland draws one
/// level float (before its LivingEntity check); stone does neither.
#[test]
#[ignore = "daily"]
fn a_landing_orb_runs_the_blocks_fall_on() {
    use horizon_client::entities::OrbMove;
    with_game(|game| {
        let slide = "minecraft:block.honey_block.slide";
        for (block, slides, draws) in [("stone", 0, 0), ("honey_block", 1, 0), ("farmland", 0, 1)] {
            assert!(game.run_command(&format!("setblock 6 200 10 {block}")));
            game.orbs.clear();
            assert!(
                game.run_command("summon experience_orb 6.5 201 10.5 {Value:1,fall_distance:2d}")
            );
            game.orbs[0].on_ground = true;
            game.audio.heard.clear();
            let mut expected = game.world.random.clone();
            for _ in 0..draws {
                expected.next_float();
            }
            game.orb_moved(
                0,
                OrbMove {
                    from: Vec3::new(6.5, 201.5, 10.5),
                    moved: Vec3::new(0.0, -0.5, 0.0),
                    in_water: false,
                },
            );
            assert_eq!(game.orbs[0].fall_distance, 0.0);
            assert_eq!(
                game.audio.heard.get(slide).copied().unwrap_or(0),
                slides,
                "{block}"
            );
            assert_eq!(
                game.world.random.clone().next_long(),
                expected.next_long(),
                "{block}: one draw per farmland landing"
            );
        }
    });
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
