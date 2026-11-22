//! A projectile that strikes an item frame hurts it (#11383): `ItemFrame.hurtServer`
//! gives the item up, an empty frame breaks, and a fixed one refuses the hit.
use crate::game::{Audio, Game, Options, Progress, Settings, execution_role, load_resources};
use horizon_client::chunk::Chunk;
use horizon_core::position::ChunkPos;
use horizon_client::arrows::Pickup;
use horizon_ecs_proto::arrow_more::{ArrowShot, Def};
use std::sync::Arc;

fn game() -> Game<'static> {
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
        crate::game::execution_role::ExecutionRole::IntegratedServer,
        &Progress::new(),
    )
    .unwrap();
    let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
    options.resources = root.clone();
    options.execution_role = execution_role::ExecutionRole::IntegratedServer;
    let res: &'static _ = Box::leak(Box::new(loaded.resources));
    let mut game = Game::new(
        res,
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
    for command in [
        "forceload add 0 0",
        "fill 0 63 0 15 63 15 stone",
        "fill 4 64 0 4 66 15 stone",
    ] {
        assert!(game.run_command(command), "{command}");
    }
    game
}

/// A plain arrow: no pickup, no fire, no punch, no effect.
fn plain_shot() -> ArrowShot {
    ArrowShot {
        pickup: Pickup::Disallowed,
        stack: crate::items::ItemStack::new("minecraft:arrow", 1),
        fx: Def {
            fire: 0,
            punch: 0.0,
            pierce: 0,
            effects: Vec::new(),
        },
        sound: "minecraft:entity.arrow.hit",
    }
}

/// An arrow from `(0.5, 64.5, z)` flying east into the wall at x = 4; its ECS id.
fn shoot(game: &mut Game<'_>, z: f64) -> u32 {
    let id = game.ecs_shoot_arrow([0.5, 64.5, z], [1.0, 0.0, 0.0], None, plain_shot(), None);
    assert_ne!(id, 0, "the arrow is in the level");
    for _ in 0..10 {
        game.tick_ecs().unwrap();
    }
    id
}

fn dropped(game: &Game<'_>, item: &str) -> usize {
    game.ecs.item_views()
        .iter()
        .filter(|d| &*d.stack.item == item)
        .count()
}

#[test]
#[ignore = "daily"]
fn an_arrow_knocks_the_item_out_then_breaks_the_frame() {
    let mut game = game();
    // On the wall's west face (Facing 4: west).
    assert!(game.run_command(
        "summon minecraft:item_frame 3.5 64.5 6.5 {Facing:4b,Item:{id:\"minecraft:diamond\",count:1}}"
    ));
    assert_eq!(game.ecs_frames().len(), 1);
    shoot(&mut game, 6.5);
    assert_eq!(game.ecs_frames().len(), 1, "the first hit leaves the frame");
    assert!(game.ecs_frames()[0].item.is_none(), "the item came out");
    assert_eq!(dropped(&game, "minecraft:diamond"), 1);
    // The empty frame breaks and drops itself.
    shoot(&mut game, 6.5);
    assert!(game.ecs_frames().is_empty());
    assert_eq!(dropped(&game, "minecraft:item_frame"), 1);
}

#[test]
#[ignore = "daily"]
fn a_fixed_frame_refuses_the_arrow() {
    let mut game = game();
    assert!(game.run_command(
        "summon minecraft:item_frame 3.5 64.5 9.5 {Facing:4b,Fixed:1b,Item:{id:\"minecraft:diamond\",count:1}}"
    ));
    let arrow = shoot(&mut game, 9.5);
    assert_eq!(game.ecs_frames().len(), 1);
    assert!(
        game.ecs_frames()[0].item.is_some(),
        "a fixed frame keeps its item"
    );
    assert_eq!(dropped(&game, "minecraft:diamond"), 0);
    // The frame took the arrow's hit and refused it, so the arrow turned back
    // (`AbstractArrow.onHitEntity`'s deflection) instead of reaching the wall.
    assert!(
        game.ecs_view(arrow)
            .is_none_or(|v| v.position[0] < 3.9),
        "the arrow went through the frame"
    );
}
