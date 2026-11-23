//! The random behind a thrown item and behind an item's sound: the thrower's
//! own (`Entity.random`, `LivingEntity.createItemStackToDrop`) or a client's
//! (`ClientPacketListener.random`), never the level's, whose draws are what
//! random ticks, leaf decay and spawns come from.
use super::*;
use crate::game::game_tests::tests::server_game;
use horizon_client::chunk::Chunk;
use horizon_client::items::ItemStack;
use std::sync::Arc;

/// A game with the player standing at 8.5 70 8.5 in a ticking chunk, with the
/// positional sounds it plays recorded.
fn standing_game() -> Game<'static> {
    let resources = Box::leak(Box::new(None));
    let mut game = server_game(resources);
    let feet = Vec3::new(8.5, 70.0, 8.5);
    // TODO(phase 2): joueur agissant = invité (corps tick le serveur : tick_items hors tour, le ramassage lit le joueur posé ici)
    game.player = Some(horizon_client::motion::PlayerMotion::new(game.motion, feet).unwrap());
    game.world
        .chunks
        .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
    game.load_ticking_neighbourhood(0, 0);
    game.update_chunk_tickets();
    assert!(game.entity_ticking_area()(8, 8));
    game.ticked_entities_through = u32::MAX;
    game.audio.capture = Some(Vec::new());
    game
}

/// `standing_game`, with its body run as a guest standing at its feet.
fn standing_acting_guest(f: impl FnOnce(&mut Game<'_>)) {
    let mut game = standing_game();
    crate::game::native_guest_turn_tests::lan_guests_on(&mut game);
    crate::game::native_guest_turn_tests::with_acting_guest_on(
        &mut game,
        Vec3::new(8.5, 70.0, 8.5),
        f,
    );
}

/// The one sound `event` played since the last call, with its volume.
fn sound(game: &mut Game<'_>, event: &str) -> (f32, f32) {
    let heard = game.audio.capture.replace(Vec::new()).unwrap_or_default();
    let sound = heard
        .iter()
        .find(|s| s.event == event)
        .unwrap_or_else(|| panic!("no {event} sound among {heard:?}"));
    (sound.volume, sound.pitch)
}

/// `Player.createItemStackToDrop(false)`: the drop key's throw is 0.3 along
/// the view (level facing, so +z) and 0.1 up, scattered by the thrower's own
/// random, which the level's draws never see.
#[test]
#[ignore = "daily"]
fn a_thrown_item_takes_the_throwers_random_not_the_levels() {
    standing_acting_guest(|game| {
    game.yaw = 0.0;
    game.pitch = 0.0;
    game.inventory[game.selected] = Some(ItemStack::new("minecraft:stone", 4));
    let level = game.world.random.state();
    game.drop_held(false);
    let thrown = game.ecs.item_views().pop().expect("the drop key threw the stack");
    let velocity = thrown.velocity;
    assert!(
        (velocity.z - 0.3).abs() <= 0.02,
        "the eye throw goes 0.3 along the view: {velocity:?}"
    );
    assert!(velocity.x.abs() <= 0.02, "{velocity:?}");
    assert!((0.0..=0.2).contains(&velocity.y), "{velocity:?}");
    assert_eq!(
        game.world.random.state(),
        level,
        "the throw drew from the level's random"
    );
    // The next throw scatters differently: the thrower's stream is live.
    game.drop_held(false);
    assert_ne!(game.ecs.item_views().pop().unwrap().velocity, velocity);
    assert_eq!(game.world.random.state(), level);
    });
}

/// `Player.dropEquipment` -> `Inventory.dropAll(randomly)`: each stack a dying
/// player lets go rises 0.2, scattered by the thrower's own random.
#[test]
#[ignore = "daily"]
fn a_death_scatter_takes_the_throwers_random_not_the_levels() {
    standing_acting_guest(|game| {
    game.inventory[0] = Some(ItemStack::new("minecraft:stone", 2));
    let level = game.world.random.state();
    game.drop_inventory(
        [8.5, 70.0, 8.5],
        &crate::game::creatures::DyingDrops {
            entity: 0,
            kind: "minecraft:player",
            player: None,
            source: None,
            recently_hit: false,
        },
    );
    let thrown = game.ecs.item_views().into_iter().next().expect("the stack was thrown");
    assert_eq!(thrown.velocity.y, f64::from(0.2f32), "a scatter rises 0.2");
    assert!(thrown.velocity.x.abs() <= 0.5 && thrown.velocity.z.abs() <= 0.5);
    assert_eq!(
        game.world.random.state(),
        level,
        "the scatter drew from the level's random"
    );
    });
}

/// `ClientPacketListener.handleTakeItemEntity` pitches the pickup from the
/// client's own random; `GiveCommand.giveItem` uses the player's. Neither is
/// the level's, which random ticks draw from.
#[test]
#[ignore = "daily"]
fn a_pickup_pitch_is_not_drawn_from_the_levels_random() {
    // TODO(phase 2): joueur agissant = invité (corps tick le serveur : tick_items et /give lisent le joueur posé par standing_game)
    let mut game = standing_game();
    game.spawn_item(
        ItemStack::new("minecraft:stone", 2),
        [8.5, 70.0, 8.5],
        [0.0; 3],
        0,
    );
    let level = game.world.random.state();
    game.tick_items();
    assert!(game.ecs.item_ids().is_empty(), "the item was picked up");
    let (volume, pitch) = sound(&mut game, "minecraft:entity.item.pickup");
    assert_eq!(volume, 0.2);
    // `(nextFloat - nextFloat) * 1.4 + 2`.
    assert!((0.6..=3.4).contains(&pitch), "pitch {pitch}");
    assert_eq!(
        game.world.random.state(),
        level,
        "the pickup drew from the level's random"
    );

    game.build_registries();
    game.set_allow_commands(true);
    let level = game.world.random.state();
    assert!(game.run_command("give @s minecraft:stone 2"));
    let (volume, pitch) = sound(&mut game, "minecraft:entity.item.pickup");
    assert_eq!(volume, 0.2);
    assert!((0.6..=3.4).contains(&pitch), "pitch {pitch}");
    assert_eq!(
        game.world.random.state(),
        level,
        "/give drew from the level's random"
    );
}

/// `Entity.lavaHurt` pitches the burn sound from the item's own random, the
/// one its constructor drew from.
#[test]
#[ignore = "daily"]
fn a_burning_items_sound_is_not_drawn_from_the_levels_random() {
    // TODO(phase 2): joueur agissant = invité (corps tick le serveur : tick_items hors tour, le joueur posé par standing_game)
    let mut game = standing_game();
    let lava = game
        .pack
        .assets
        .registry
        .default_state("minecraft:lava")
        .unwrap();
    game.world.set(12, 70, 8, lava).unwrap();
    game.spawn_item(
        ItemStack::new("minecraft:stone", 1),
        [12.5, 70.0, 8.5],
        [0.0; 3],
        0,
    );
    // `Entity.lavaHurt` sounds on the tenth tick of a contact that leaves the
    // item alive (`should_playLavaHurtSound`), so the item is about to be 10.
    game.ecs.set_item_ticks(game.ecs.item_ids()[0], 9);
    let level = game.world.random.state();
    game.tick_items();
    let (volume, pitch) = sound(&mut game, "minecraft:entity.generic.burn");
    assert_eq!(volume, 0.4);
    // `2.0 + nextFloat * 0.4`.
    assert!((2.0..2.4).contains(&pitch), "pitch {pitch}");
    assert_eq!(
        game.world.random.state(),
        level,
        "the burn sound drew from the level's random"
    );
}
