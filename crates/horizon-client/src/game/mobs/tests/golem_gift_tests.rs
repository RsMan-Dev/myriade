//! An iron golem's flower gift (`OfferFlowerGoal.stop`): the copper golem it
//! offered the poppy to wears it on its empty antenna (`setItemSlot`) as a
//! guaranteed drop (`setGuaranteedDrop`: drop chance 2.0 for that slot). The
//! gift goes through the game's own `MobEvent::Gift` handler.
use super::super::*;
use horizon_client::{chunk::Chunk, mobs::MobEvent};
use horizon_core::position::ChunkPos;
use std::sync::Arc;

const EMPTY: &str = "0000609b-0000-0000-0000-000000000001";

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
    for command in [
        "forceload add 0 0",
        "fill 0 199 0 15 199 15 stone",
        "tick freeze",
        // Waxed (`next_weather_age` -2): no statue while the test runs.
        "summon copper_golem 2.5 200 2.5 {NoAI:1b,PersistenceRequired:1b,next_weather_age:-2L,UUID:[I;24731,0,0,1]}",
    ] {
        assert!(game.run_command(command), "{command}");
    }
    game
}

fn id(game: &Game<'_>, uuid: &str) -> u32 {
    match game.target_with_uuid(horizon_client::save::uuid_from_text(uuid).unwrap()) {
        Some(crate::game::commands::Target::Mob(id)) => id,
        other => panic!("{other:?}"),
    }
}

/// `OfferFlowerGoal.stop` for the golem whose UUID is `uuid`.
fn gift(game: &mut Game<'_>, uuid: &str) {
    gift_item(game, uuid, "minecraft:poppy");
}

fn gift_item(game: &mut Game<'_>, uuid: &str, item: &'static str) {
    let target = id(game, uuid);
    game.handle_mob_events(vec![MobEvent::Gift { target, item }])
        .unwrap();
}

fn holds(game: &mut Game<'_>, uuid: &str, nbt: &str) -> bool {
    game.run_command(&format!(
        "execute if entity @e[nbt={{UUID:{}}},nbt={nbt}]",
        ints(uuid)
    ))
}

fn ints(uuid: &str) -> String {
    let u = horizon_client::save::uuid_from_text(uuid).unwrap();
    format!("[I;{},{},{},{}]", u[0], u[1], u[2], u[3])
}

#[test]
#[ignore = "daily"]
fn a_gifted_poppy_is_a_guaranteed_drop() {
    let mut resources = None;
    let mut game = game(&mut resources);
    assert!(
        !game.run_command(&format!(
            "execute if items entity {EMPTY} saddle minecraft:poppy"
        )),
        "the antenna starts empty"
    );
    assert!(!holds(&mut game, EMPTY, "{drop_chances:{saddle:2.0f}}"));
    gift(&mut game, EMPTY);
    assert!(
        game.run_command(&format!(
            "execute if items entity {EMPTY} saddle minecraft:poppy"
        )),
        "the poppy is on the antenna"
    );
    // The same check as the vanilla capture (tools: probes/golem-gift).
    assert!(
        holds(
            &mut game,
            EMPTY,
            "{equipment:{saddle:{id:\"minecraft:poppy\"}},drop_chances:{saddle:2.0f}}"
        ),
        "Mob.setGuaranteedDrop(antenna)"
    );
}
