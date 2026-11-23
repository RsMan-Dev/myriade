//! `/kill` on a mob: `LivingEntity.kill` hurts it with `genericKill`, so
//! `LivingEntity.die` runs before the command returns and posts
//! `gameEvent(ENTITY_DIE)` at the mob (tools/test_kill_die_vibration_parity.py
//! measures the same on vanilla 26.3: the sensor's listener holds the event
//! before any world tick).
use super::*;
use myriade_engine::chunk::Chunk;
use horizon_core::position::ChunkPos;
use std::sync::Arc;

pub(super) fn game(test: impl FnOnce(&mut Game<'_>)) {
    crate::game::native_entity_refs::tests::with_game(|game| {
        game.build_registries();
        game.load_server_loot();
        game.set_allow_commands(true);
        game.world
            .chunks
            .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
        game.block_ticks
            .add_empty_chunk(ChunkPos::new(0, 0))
            .unwrap();
        assert!(game.run_command("fill 0 99 0 15 99 15 stone"));
        // The sensor joins the listeners through the level's own index.
        game.sculk_prepare();
        assert!(game.run_command("setblock 8 100 8 sculk_sensor"));
        game.sculk_sync_index();
        assert!(game.run_command(
        "summon pig 5.5 100 8.5 {NoAI:1b,NoGravity:1b,PersistenceRequired:1b,UUID:[I;593,0,0,1]}"
    ));
        test(game);
    });
}


