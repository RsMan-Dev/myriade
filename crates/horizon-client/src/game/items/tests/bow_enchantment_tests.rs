//! `BowItem.releaseUsing` through `ProjectileWeaponItem.draw` and `shoot`: the
//! bow's enchantment components decide the arrows it fires, and the ammunition
//! is whatever `#minecraft:arrows` lists.
use crate::game::{Audio, Game, Options, Progress, Settings, load_resources};
use horizon_client::items::ItemStack;

/// A game on the prepared resources with a creative player (as `crossbow.rs`).
fn game_with_player(root: &std::path::Path) -> Game<'static> {
    let settings = Settings::default();
    let loaded = load_resources(
        horizon_assets::resource_source::ResourceRoot::directory(root.to_path_buf()),
        Vec::new(),
        None,
        root,
        &settings,
        std::time::Instant::now(),
        crate::game::execution_role::ExecutionRole::IntegratedServer,
        &Progress::new(),
    )
    .unwrap();
    let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
    options.resources = root.to_path_buf();
    options.execution_role = crate::game::execution_role::ExecutionRole::IntegratedServer;
    let res: &'static _ = Box::leak(Box::new(loaded.resources));
    let mut game = Game::new(
        res,
        loaded.pack,
        options,
        settings,
        Audio::default(),
    )
    .unwrap();
    game.creative = true;
    game.player = Some(
        horizon_client::motion::PlayerMotion::new(
            game.motion,
            horizon_core::geometry::Vec3::new(10.5, 64.0, -3.25),
        )
        .unwrap(),
    );
    // A still player: `known_player_movement` is the last tick's step.
    game.previous_position = game.player.as_ref().expect("a player").position;
    game.audio.capture = Some(Vec::new());
    game
}

/// `ProjectileWeaponItem.ARROW_ONLY` is `itemStack.is(ItemTags.ARROWS)`: an
/// item a data pack adds to `#minecraft:arrows` is bow and crossbow ammunition.
/// Before, `is_arrow` listed the three vanilla ids.
#[test]
fn an_item_a_pack_adds_to_the_arrows_tag_is_ammunition() {
    use crate::game::crossbow::projectile_slot;
    use crate::game::shield::OFFHAND_SLOT;
    use horizon_client::items::Items;
    let tag = |values: &[&str]| {
        std::collections::HashMap::from([(
            "minecraft:arrows".to_owned(),
            values.iter().map(|v| (*v).to_owned()).collect(),
        )])
    };
    let items = Items::for_tests(tag(&["minecraft:arrow", "test:bone_arrow"]));
    let mut inventory = vec![None; 36];
    inventory[9] = Some(ItemStack::new("test:bone_arrow", 4));
    let hands = [(OFFHAND_SLOT, None), (0, None)];
    assert_eq!(
        projectile_slot("minecraft:bow", hands, &inventory, &items),
        Some(9)
    );
    // An item the tag does not list stays out of the bow.
    inventory[9] = Some(ItemStack::new("test:pebble", 4));
    assert_eq!(
        projectile_slot("minecraft:bow", hands, &inventory, &items),
        None
    );
}
