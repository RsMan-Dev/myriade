//! `AbstractThrownPotion.onHit` reads the stack's whole `potion_contents`: the
//! potion's effects and the stack's `custom_effects`, the colour they mix to
//! (or `custom_color`) and its `potion_duration_scale`.
use super::*;
use crate::game::game_tests::tests::server_game;

/// A server's game with its lan guests placed, the acting one standing at the origin's block.
fn player_game(resources: &mut Option<std::sync::Arc<crate::game::Resources>>) -> Game<'_> {
    let mut game = server_game(resources);
    crate::game::native_guest_turn_tests::lan_guests_on(&mut game);
    game.build_registries();
    game.set_allow_commands(true);
    game
}

/// A stack with `custom_effects` of `(effect, duration, amplifier)` on top of
/// whatever potion it carries, as `PotionContents.customEffects`.
fn with_custom_effects(mut stack: ItemStack, effects: &[(&str, i32, i32)]) -> ItemStack {
    let list = effects
        .iter()
        .map(|(id, duration, amp)| {
            Tag::compound([
                ("id", Tag::String(format!("minecraft:{id}").as_str().into())),
                ("duration", Tag::Int(*duration)),
                ("amplifier", Tag::Int(*amp)),
            ])
        })
        .collect();
    stack
        .data_mut()
        .potion_extra
        .push(("custom_effects".into(), Tag::List(list)));
    stack
}

/// `PotionContents.is(PotionTags.DOUSES_FIRE)` needs `isPotionWithoutCustomEffects`:
/// water carrying a custom effect leaves a lit campfire alone, while plain
/// water still puts it out.
#[test]
#[ignore = "daily"]
fn a_water_potion_with_custom_effects_does_not_douse_fire() {
    let mut resources = None;
    let mut game = server_game(&mut resources);
    let pos = [2, 4, 3];
    let campfire = game
        .pack
        .assets
        .registry
        .default_state("minecraft:campfire")
        .unwrap();
    game.world.set(pos[0], pos[1], pos[2], campfire).unwrap();
    let lit = |game: &Game<'_>| {
        game.pack
            .assets
            .registry
            .state(game.world.block(pos[0], pos[1], pos[2]))
            .and_then(|s| s.properties.get("lit"))
            .is_some_and(|v| v == "true")
    };
    assert!(lit(&game), "the campfire starts lit");
    // The potion hits the top of the block under the campfire, so the cell
    // `AbstractThrownPotion.douseFire` looks at first is the campfire's.
    let hit = Some(([2, 3, 3], Direction::Up));
    let at = Vec3::new(2.5, 3.5, 3.5);
    let throw = |game: &mut Game<'_>, stack: &ItemStack| {
        game.splash(
            stack,
            at,
            None,
            hit,
            0,
            &PotionOwner::default(),
            true,
            Vec3::ZERO,
        )
    };
    let water = ItemStack::new("minecraft:splash_potion", 1).with_potion("minecraft:water");
    throw(
        &mut game,
        &with_custom_effects(water.clone(), &[("slowness", 1800, 0)]),
    );
    assert!(
        lit(&game),
        "water with custom effects is not in #douses_fire"
    );
    throw(&mut game, &water);
    assert!(!lit(&game), "plain water puts the campfire out");
}
