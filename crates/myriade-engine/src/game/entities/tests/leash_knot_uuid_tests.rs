//! A mob or boat whose `leash` names a leash knot by UUID is tied to that knot
//! (#13113): `Leashable.restoreLeashFromSave` finds it with
//! `ServerLevel.getEntity`, as any other holder.
use super::*;
use myriade_engine::chunk::Chunk;
use horizon_core::position::ChunkPos;
use std::sync::Arc;

fn game<'a>(resources: &'a mut Option<std::sync::Arc<crate::game::Resources>>) -> Game<'a> {
    let mut game = crate::game::game_tests::tests::server_game(resources);
    game.build_registries();
    game.set_allow_commands(true);
    game.world
        .chunks
        .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
    game.block_ticks
        .add_empty_chunk(ChunkPos::new(0, 0))
        .unwrap();
    game.load_ticking_neighbourhood(0, 0);
    for command in [
        "forceload add 0 0",
        "fill 0 63 0 15 63 15 stone",
        "setblock 4 64 6 minecraft:oak_fence",
        "summon minecraft:leash_knot 4.5 64 6.5 {UUID:[I;7193,0,0,1]}",
    ] {
        assert!(game.run_command(command), "{command}");
    }
    game
}

#[test]
#[ignore = "daily"]
fn a_boat_leashed_to_a_knots_uuid_is_tied_to_its_post() {
    let mut resources = None;
    let mut game = game(&mut resources);
    assert!(game.run_command("summon minecraft:oak_boat 2.5 64 9.5 {leash:{UUID:[I;7193,0,0,1]}}"));
    for _ in 0..150 {
        game.tick_server(false).unwrap();
    }
    let boat = *game.ecs.boat_ids().last().unwrap();
    assert_eq!(
        game.ecs_view(boat).unwrap().leash,
        Some(horizon_ecs_proto::leash::Holder::Knot([4, 64, 6]))
    );
}
