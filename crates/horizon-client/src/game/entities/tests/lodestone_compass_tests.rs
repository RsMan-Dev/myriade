//! Only `CompassItem.inventoryTick` ticks a `LodestoneTracker`, so any other
//! item carrying the component keeps it.
use crate::game::game_tests::tests::server_game;
use horizon_client::items::ItemStack;
use horizon_core::nbt::Tag;

/// The lodestone at `pos` (the `target` of a `LodestoneTracker`).
fn tracker(pos: [i32; 3]) -> Tag {
    Tag::compound([(
        "target",
        Tag::compound([
            ("dimension", Tag::String("minecraft:overworld".into())),
            ("pos", Tag::IntArray(pos.to_vec())),
        ]),
    )])
}

#[test]
#[ignore = "daily"]
fn only_a_compass_ticks_its_lodestone_tracker() {
    let mut resources = None;
    let mut game = server_game(&mut resources);
    // The resident chunk (0, 0) holds no lodestone at 5 70 5: the target is gone.
    for (slot, item) in [(0, "minecraft:stick"), (1, "minecraft:compass")] {
        let mut stack = ItemStack::new(item, 1);
        stack
            .data_mut()
            .set_other("minecraft:lodestone_tracker", Some(tracker([5, 70, 5])));
        *game.stack_in_mut(slot) = Some(stack);
    }
    game.tick_lodestone_compasses();
    let target = |slot: usize| {
        game.stack_in(slot)
            .and_then(|stack| stack.data())
            .and_then(|data| data.other("minecraft:lodestone_tracker"))
            .and_then(|tracker| tracker.get("target"))
            .and_then(|target| target.get("pos").cloned())
    };
    // `Item.inventoryTick` is empty: a stick keeps what it carries.
    assert_eq!(target(0), Some(Tag::IntArray(vec![5, 70, 5])));
    // `CompassItem.inventoryTick` -> `LodestoneTracker.tick` clears it.
    assert_eq!(target(1), None);
}
