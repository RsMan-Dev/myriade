//! What a shield does with a `bypassed_by` list.
use crate::game::*;
use super::OFFHAND_SLOT;
use horizon_client::items::ItemStack;
use horizon_core::nbt::Tag;

/// A server's game with the player standing at (0.5, 64, 0.5), or `None`
/// without prepared resources.
fn game() -> Option<Game<'static>> {
    std::env::var_os("HORIZON_TEST_RESOURCES")?;
    let mut game = crate::game::native_machines::tests::server_game();
    game.build_registries();
    Some(game)
}

/// `game()`, with the body run as a guest standing at (0.5, 64, 0.5); skipped without the resources.
fn with_acting_guest_game(f: impl FnOnce(&mut Game<'_>)) {
    let Some(mut game) = game() else {
        return;
    };
    crate::game::native_guest_turn_tests::lan_guests_on(&mut game);
    crate::game::native_guest_turn_tests::with_acting_guest_on(
        &mut game,
        Vec3::new(0.5, 64.0, 0.5),
        f,
    );
}

/// `stack` carrying a `blocks_attacks` of its own whose `bypassed_by` is `bypassed`.
fn bypassing_stack(stack: ItemStack, bypassed: Tag) -> ItemStack {
    let mut stack = stack;
    stack.data_mut().other.push((
        "minecraft:blocks_attacks".into(),
        Tag::compound([("bypassed_by", bypassed)]),
    ));
    stack
}

/// `RegistryCodecs.holderSet` takes a tag, one id or a list of them, so a
/// `bypassed_by` in any of those forms lets that damage type through.
#[test]
fn a_bypassed_by_list_lets_the_hit_through() {
    with_acting_guest_game(|game| {
    let forms = [
        Tag::List(vec![Tag::String("minecraft:mob_attack".into())]),
        Tag::String("minecraft:mob_attack".into()),
        Tag::String("#minecraft:panic_causes".into()),
    ];
    for bypassed in forms {
        game.offhand = Some(bypassing_stack(
            ItemStack::new("minecraft:shield", 1),
            bypassed.clone(),
        ));
        game.using = Some((OFFHAND_SLOT, 0));
        let blocking = game
            .player_blocking(Some([0.5, 64.0, 2.5]), "minecraft:mob_attack", false)
            .expect("the shield is up");
        assert!(blocking.bypassed, "{bypassed:?} bypasses the mob attack");
        assert_eq!(blocking.blocked(5.0), 0.0, "{bypassed:?} blocks nothing");
    }
    // A list that does not name it still blocks.
    game.offhand = Some(bypassing_stack(
        ItemStack::new("minecraft:shield", 1),
        Tag::List(vec![Tag::String("minecraft:fall".into())]),
    ));
    game.using = Some((OFFHAND_SLOT, 0));
    // In front of the player (yaw 0 faces +z), inside the shield's 90 degrees.
    let blocking = game
        .player_blocking(Some([0.5, 64.0, 2.5]), "minecraft:mob_attack", false)
        .expect("the shield is up");
    assert!(!blocking.bypassed);
    assert!(blocking.blocked(5.0) > 0.0);
    });
}
