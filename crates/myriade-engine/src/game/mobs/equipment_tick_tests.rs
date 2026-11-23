//! `LivingEntity.aiStep` -> `EntityEquipment.tick` -> `CompassItem.inventoryTick` for mobs, and
//! the compass-only rule a player's `Inventory.tick` shares with them (arismoko/horizon#3221).
use super::Game;
use myriade_engine::{items::ItemStack, motion::PlayerMotion};
use horizon_core::{geometry::Vec3, nbt::Tag, position::BlockPos};

/// A server with chunk (0, 0) loaded and entity-ticking, a stone floor at y 63 and lodestones
/// at (5, 64, 5) and (7, 64, 7).
fn lodestone_world(resources: &mut Option<std::sync::Arc<crate::game::Resources>>) -> Game<'_> {
    let mut game = crate::game::game_tests::tests::server_game(resources);
    game.build_registries();
    game.set_allow_commands(true);
    game.level.health = 20.0;
    game.player = Some(PlayerMotion::new(game.motion, Vec3::new(14.5, 64.0, 14.5)).unwrap());
    game.block_ticks
        .add_empty_chunk(BlockPos::new(0, 0, 0).chunk())
        .unwrap();
    assert!(game.run_command("forceload add 0 0"));
    assert!(game.run_command("fill 0 63 0 15 63 15 stone"));
    assert!(game.run_command("setblock 5 64 5 lodestone"));
    assert!(game.run_command("setblock 7 64 7 lodestone"));
    game
}

fn tracker(dimension: &str, pos: [i32; 3]) -> String {
    let [x, y, z] = pos;
    format!(
        "minecraft:lodestone_tracker={{target:{{dimension:\"{dimension}\",pos:[I;{x},{y},{z}]}},tracked:true}}"
    )
}

/// The stack's `minecraft:lodestone_tracker`.
fn lodestone_tracker(stack: &ItemStack) -> &Tag {
    stack
        .data()
        .and_then(|d| d.other("minecraft:lodestone_tracker"))
        .expect("the stack keeps its lodestone_tracker")
}

/// `{target: {dimension, pos}}` of a tracker, as vanilla's codec writes it.
fn target(stack: &ItemStack) -> Option<(String, Vec<i32>)> {
    let target = lodestone_tracker(stack).get("target")?;
    let dimension =
        myriade_engine::save::text(target.get("dimension")).expect("a target with a dimension");
    let Some(Tag::IntArray(pos)) = target.get("pos") else {
        panic!("a target without a pos");
    };
    Some((dimension.to_string(), pos.clone()))
}

/// `tracked` reads true when it is left out (`Codec.BOOL.optionalFieldOf("tracked", true)`).
fn tracked(stack: &ItemStack) -> bool {
    myriade_engine::save::number(lodestone_tracker(stack).get("tracked")).is_none_or(|v| v != 0)
}

#[test]
#[ignore = "daily"]
fn a_players_non_compass_keeps_its_lodestone_tracker() {
    let mut resources = None;
    let mut game = lodestone_world(&mut resources);
    let target_5 = tracker("minecraft:overworld", [5, 64, 5]);
    assert!(game.run_command(&format!(
        "item replace entity @s weapon.mainhand with stick[{target_5}] 1"
    )));
    assert!(game.run_command(&format!(
        "item replace entity @s weapon.offhand with compass[{target_5}] 1"
    )));
    assert!(game.run_command("setblock 5 64 5 stone"));
    game.tick_lodestone_compasses();
    let held = game.stack_in(0).unwrap();
    assert_eq!(&*held.item, "minecraft:stick");
    // Only `CompassItem` overrides `inventoryTick` with `LodestoneTracker.tick`.
    assert_eq!(
        target(held),
        Some(("minecraft:overworld".to_owned(), vec![5, 64, 5]))
    );
    let offhand = game.stack_in(crate::game::shield::OFFHAND_SLOT).unwrap();
    assert_eq!(target(offhand), None);
    assert!(tracked(offhand));
}
