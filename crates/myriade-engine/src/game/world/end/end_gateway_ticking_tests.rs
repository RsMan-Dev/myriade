//! `EndGatewayBlock.entityInside` comes from the entity's own tick
//! (`Entity.applyEffectsFromBlocks`), and `ServerLevel.tick` ticks only the
//! entities in entity-ticking chunks: past the simulation distance an item in
//! an End gateway stays where it is, and only the ticking one goes in.

use crate::game::Dim;
use crate::game::game_tests::tests::server_game;
use myriade_engine::chunk::Chunk;
use myriade_engine::items::ItemStack;
use std::sync::Arc;


#[test]
#[ignore = "daily"]
fn items_past_the_simulation_distance_enter_no_end_gateway() {
    let mut resources = None;
    let mut game = server_game(&mut resources);
    game.dim = Dim::End;
    game.settings.simulation_distance = 4;
    game.settings.view_distance = 12;
    let gateway = game
        .pack
        .assets
        .registry
        .default_state("minecraft:end_gateway")
        .unwrap();
    // The player in chunk (0, 0); chunk (2, 0) is inside the simulation
    // distance, chunk (8, 0) loaded and in view but past it. In each, an item
    // lies in an End gateway block.
    crate::game::native_guest_turn_tests::lan_guests_on(&mut game);
    for (cx, x) in [(2, 40), (8, 136)] {
        game.world
            .chunks
            .insert((cx, 0), Arc::new(Chunk::empty(cx, 0)));
        game.load_ticking_neighbourhood(cx, 0);
        game.world.set(x, 64, 8, gateway).unwrap();
        let at = [f64::from(x) + 0.5, 64.2, 8.5];
        let item = game.spawn_item(ItemStack::new("minecraft:white_carpet", 1), at, [0.0; 3], 0).expect("an item");
        game.ecs.set_no_gravity(item, true);
    }
    game.update_chunk_tickets();
    let ticking = game.entity_ticking_area();
    assert!(ticking(40, 8), "chunk (2, 0) ticks entities");
    assert!(
        !ticking(136, 8),
        "chunk (8, 0) is past the simulation distance"
    );
    game.find_end_gateway_portals();
    assert!(
        game.end.gateways.contains_key(&1),
        "the ticking item went into its gateway"
    );
    assert!(
        !game.end.gateways.contains_key(&2),
        "the item past the simulation distance never entered its gateway"
    );
    // It waits there: the gateways' pass leaves it alone.
    game.handle_end_gateway_portals();
    assert!(game.ecs.item_views().iter().any(|d| d.id == 2));
}

/// `TheEndGatewayBlockEntity.portalTick` counts the Age up in the block entity
/// itself, so `/data get block` reads 0, then 5, then 25 (#13202), also far
/// from the player, where nothing draws the gateway.
#[test]
#[ignore = "daily"]
fn a_ticking_gateway_counts_its_age_in_its_block_entity() {
    use myriade_engine::containers::BlockEntity;
    let mut game_resources = None;
    let mut game = server_game(&mut game_resources);
    game.build_registries();
    game.set_allow_commands(true);
    game.dim = Dim::End;
    crate::game::native_guest_turn_tests::lan_guests_on(&mut game);
    game.world
        .chunks
        .insert((2, 0), Arc::new(Chunk::empty(2, 0)));
    // A dedicated server with nobody on it: the console runs the command, on
    // the command registries (as block_entity_anims' gateway test does).
    game.build_registries();
    assert!(
        game.as_console(|game| game.run_command("setblock 40 64 8 minecraft:end_gateway{Age:0L}"))
    );
    game.update_chunk_tickets();
    assert!(game.block_ticking_area()(40, 8));
    // What `/data get block` and the chunk save encode (`TheEndGatewayBlockEntity.saveAdditional`).
    let age = |game: &super::super::Game<'_>| match game.world.block_entities.get(&(40, 64, 8)) {
        Some(BlockEntity::EndGateway(g)) => Some(g.age),
        _ => None,
    };
    assert_eq!(age(&game), Some(0));
    for _ in 0..5 {
        game.tick_block_entities().unwrap();
    }
    assert_eq!(age(&game), Some(5));
    for _ in 0..20 {
        game.tick_block_entities().unwrap();
    }
    assert_eq!(age(&game), Some(25));
}
