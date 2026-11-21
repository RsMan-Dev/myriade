//! `AbstractMountInventoryMenu.stillValid` asks
//! `player.isWithinEntityInteractionRange(mount, 4.0)`, the player's eyes to
//! the mount's box against the entity interaction range plus 4: a donkey's
//! menu stays open at 6.5 blocks and closes past 7, not at the 4.5 a
//! centre-to-centre check gives. A server's guest is checked by
//! `Game::lan_mount_valid`, the window's own copy by `Game::tick_mount_menu`.
use super::super::*;
use horizon_client::{
    lan::{Joined, PlayerSender, guest::Guest},
    mobs::{Mob, Species},
    motion::PlayerMotion,
};
use horizon_core::geometry::Vec3;
use horizon_protocol::packets::configuration::ClientInformation;
use std::sync::{Arc, atomic::AtomicI32, mpsc};

const DONKEY: u32 = 7;

/// A tamed, chested donkey at `(8.5, 64, 8.5)` and a guest whose eyes are
/// `gap` blocks east of its box (the box's east face is at 8.5 + its half
/// width, and the eyes sit inside its height).
fn guest_beside_a_donkey(gap: f64) -> (Game<'static>, Guest) {
    let resources = Box::leak(Box::new(None));
    let mut game = crate::game::game_tests::tests::server_game(resources);
    let kind = game
        .res
        .mob_types()
        .get("minecraft:donkey")
        .expect("a donkey type")
        .clone();
    let mut mob = Mob::new(DONKEY, kind, Vec3::new(8.5, 64.0, 8.5), 0.0);
    mob.extra.tame = true;
    mob.extra.chested = true;
    game.mobs.push(mob);
    let (tx, _rx) = mpsc::channel();
    let mut guest = Guest::new(
        Joined {
            conn: 2,
            uuid: 2,
            name: "ReachGuest".into(),
            properties: Vec::new(),
            info: ClientInformation::new(2),
            brand: "test".into(),
            sender: PlayerSender::new(tx, Arc::default()),
            latency: Arc::new(AtomicI32::new(0)),
            address: "127.0.0.1:25565".parse().unwrap(),
            expected: false,
        },
        2,
        [8.5, 64.0, 8.5],
        2,
    );
    guest.dimension = "minecraft:overworld".into();
    guest.placed = true;
    guest.loaded = true;
    guest.position = [box_face(&game) + gap, 64.0, 8.5];
    (game, guest)
}

/// The east face of the donkey's box at `(8.5, 64, 8.5)`.
fn box_face(game: &Game<'_>) -> f64 {
    8.5 + f64::from(game.mobs[0].width()) / 2.0
}

/// A server checks its guest's mount menu against the guest's own entity
/// interaction range (`Player.isWithinEntityInteractionRange(mount, 4.0)`):
/// 3 + 4 blocks in survival, 5 + 4 in creative.
#[test]
#[ignore = "daily"]
fn a_guests_mount_menu_holds_to_its_own_entity_interaction_range_plus_four() {
    let (game, mut guest) = guest_beside_a_donkey(6.5);
    assert!(
        game.lan_mount_valid(&guest, DONKEY),
        "the menu closed 6.5 blocks from the donkey's box"
    );
    guest.position[0] = box_face(&game) + 7.5;
    assert!(
        !game.lan_mount_valid(&guest, DONKEY),
        "the menu stayed open 7.5 blocks from the donkey's box"
    );
    // Creative's entity interaction range is 5, so the menu holds to 9.
    guest.game_mode = 1;
    guest.position[0] = box_face(&game) + 8.0;
    assert!(
        game.lan_mount_valid(&guest, DONKEY),
        "a creative guest's menu closed 8 blocks from the donkey's box"
    );
    guest.position[0] = box_face(&game) + 9.5;
    assert!(
        !game.lan_mount_valid(&guest, DONKEY),
        "a creative guest's menu stayed open 9.5 blocks from the donkey's box"
    );
}
