//! `ItemEntity.areMergable` and `merge` on stacks with their own
//! `max_stack_size`: dropped items merge up to `ItemStack.getMaxStackSize`,
//! which is the stack's own size and not the item's default.
use super::native_machines::tests::server_game;
use super::*;
use horizon_core::nbt::Tag;

/// Stone with its own `max_stack_size`, or with the component removed
/// (`!minecraft:max_stack_size`, vanilla's `getOrDefault` default of one).
fn stone(size: Option<i32>, count: u32) -> ItemStack {
    let mut stack = ItemStack::new("minecraft:stone", count);
    let data = stack.data_mut();
    match size {
        Some(n) => data.set_other("minecraft:max_stack_size", Some(Tag::Int(n))),
        None => data.set_other(
            "!minecraft:max_stack_size",
            Some(Tag::Compound(Default::default())),
        ),
    };
    stack.tidy();
    stack
}

/// `ItemEntity.areMergable` and `merge`: two stacks that do not fit one stack
/// of their own size stay apart, and the rest merge.
#[test]
fn items_merge_only_up_to_their_own_max_stack_size() {
    if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
        return;
    }
    let mut game = server_game();
    game.ecs = game.test_ecs_level();
    let mut ids: Vec<u32> = Vec::new();
    let counts = |game: &Game<'_>, ids: &[u32]| {
        let mut counts: Vec<u32> = ids.iter().filter_map(|&id| game.ecs.item_stack(id)).map(|s| s.count).collect();
        counts.sort_unstable();
        counts
    };
    let at = [0.5, 70.0, 0.5];
    let spawn = |game: &mut Game<'_>, ids: &mut Vec<u32>, stacks: &[ItemStack]| {
        for &id in ids.iter() {
            if let Some(e) = game.ecs.entity_of(id) {
                game.ecs.despawn(e);
            }
        }
        *ids = stacks.iter().map(|s| game.ecs_drop_after(s.clone(), at, [0.0; 3], 0).unwrap()).collect();
        // `ItemEntity.tick` looks for a partner every 2 ticks while the stack changes block (every 40 at rest): a stack falling from
        // y = 70 crosses its second block at the tenth tick, so twelve ticks hold a look.
        for _ in 0..12 {
            game.ecs_with(|ecs, mw| ecs.step(mw));
        }
    };
    // Two and one of a stone that holds four fit one stack, so they merge.
    spawn(&mut game, &mut ids, &[stone(Some(4), 2), stone(Some(4), 1)]);
    assert_eq!(counts(&game, &ids), [3]);
    // Two and three add up to five, so `areMergable` refuses.
    spawn(&mut game, &mut ids, &[stone(Some(4), 2), stone(Some(4), 3)]);
    assert_eq!(counts(&game, &ids), [2, 3]);
    // A removed component holds one, so two of them stay apart as well.
    spawn(&mut game, &mut ids, &[stone(None, 1), stone(None, 1)]);
    assert_eq!(counts(&game, &ids), [1, 1]);
}
