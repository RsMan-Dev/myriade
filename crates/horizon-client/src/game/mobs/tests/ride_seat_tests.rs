//! `/ride` (`RideCommand.mount`): where a rider sits on its mount
//! (`Entity.getPassengerRidingPosition`, the vehicle type's own
//! `passengerAttachments`), a mob as a minecart's passenger, and the entity
//! vanilla lets nobody ride.
use super::tests::server_game;
use super::*;
use crate::game::commands::Target;
use crate::game::minecarts::CartKind;
use horizon_client::motion::PlayerMotion;

/// A rideable minecart with id `id` at `at`.
fn cart_at(game: &mut Game<'_>, id: u32, at: [f64; 3]) {
    game.ecs_with(|ecs, mw| ecs.spawn_cart_id(mw, CartKind::Rideable.id(), at, 0.0, &mut || id))
        .expect("a rideable minecart spawns");
}

/// `Marker.couldAcceptPassenger` is false, so `/ride`'s forced
/// `startRiding(vehicle, true, true)` fails there with the generic mount
/// failure; a display, which vanilla's `Entity` does let ride, still takes the
/// player on its zero-height passenger point.
#[test]
fn a_marker_takes_no_rider() {
    let mut resources = None;
    let Some(mut game) = server_game(&mut resources) else {
        return;
    };
    for (id, kind) in [(9, "minecraft:marker"), (10, "minecraft:item_display")] {
        game.load_other_as(id, kind, [0.5, 64.0, 0.5]);
    }
    let failed = game
        .ride_mount(Target::Player, Target::Other(9))
        .expect_err("a marker takes no rider");
    assert_eq!(
        failed.key, "commands.ride.mount.failure.generic",
        "vanilla's `ERROR_MOUNT_FAILED`"
    );
    assert_eq!(game.riding, None, "the player stays off the marker");
    game.player =
        Some(PlayerMotion::new(game.motion, Vec3::new(0.5, 64.0, 0.5)).expect("a player to ride"));
    game.ride_mount(Target::Player, Target::Other(10))
        .expect("a display takes a rider");
    assert_eq!(game.riding, Some(10));
    let p = game.player.as_ref().expect("a player").position;
    assert_eq!(
        p.y, 63.4,
        "the display's seat is 0.6 below its feet: {}",
        p.y
    );
}
