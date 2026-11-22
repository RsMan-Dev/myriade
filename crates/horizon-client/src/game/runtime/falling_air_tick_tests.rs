use horizon_client::chunk::Chunk;
use horizon_core::position::ChunkPos;
use std::sync::Arc;

#[test]
#[ignore = "daily"]
fn loaded_air_is_discarded_on_first_actual_game_tick() {
    crate::game::native_entity_refs::tests::with_game(|game| {
        game.build_registries();
        game.set_allow_commands(true);
        game.world
            .chunks
            .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
        game.block_ticks
            .add_empty_chunk(ChunkPos::new(0, 0))
            .unwrap();
        game.load_ticking_neighbourhood(0, 0);
        assert!(game.run_command("forceload add 0 0"));
        assert!(game.run_command("fill 0 199 0 15 199 15 stone"));
        assert!(game.res.facts().is_air(game.world.block(4, 204, 4)));
        assert!(game.res.facts().is_air(game.world.block(8, 204, 4)));
        for (last, state, x) in [(1, "air", 4), (2, "sand", 8)] {
            assert!(game.run_command(&format!(
                "summon falling_block {x}.5 204 4.5 {{UUID:[I;97,0,0,{last}],BlockState:\"minecraft:{state}\",Time:0,NoGravity:1b,Motion:[0d,0d,0d]}}"
            )));
        }
        let ids = game.ecs.species_ids("minecraft:falling_block");
        assert_eq!(ids.len(), 2, "both actual command loads must be admitted");
        assert!(!game.res.facts().is_air(game.world.block(4, 199, 4)));
        let clock = game.world.game_time;
        game.tick().unwrap();
        assert_eq!(game.world.game_time, clock + 1);
        assert_eq!(game.ecs.species_ids("minecraft:falling_block"), [ids[1]], "admitted air must disappear on its first tick");
    });
}
