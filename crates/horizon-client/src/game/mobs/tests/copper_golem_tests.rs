//! `CarvedPumpkinBlock.trySpawnGolem` finds the copper golem's block of
//! `#minecraft:copper` in any of the six directions from the pumpkin
//! (`BlockPattern.find` tries every forwards and up pair), and
//! `PumpkinBlock.useItemOn` wears the shears in the hand that carved and
//! reports that hand's break (`hurtAndBreak(1, player, hand.asEquipmentSlot())`).
use super::super::*;
use crate::game::native_machines::tests::server_game;
use horizon_client::items::ItemStack;
use horizon_client::motion::PlayerMotion;
use horizon_core::geometry::Vec3;

// TODO(phase 2): joueur local retiré — à faire agir par un joueur connecté (as_guest).
#[test]
#[ignore = "daily"]
fn carving_with_off_hand_shears_breaks_the_off_hand() {
    let mut game = server_game();
    game.set_mode_id(0);
    game.player = Some(PlayerMotion::new(game.motion, Vec3::new(8.5, 64.0, 8.5)).unwrap());
    // A guest's break is queued with its entity event; the local player's is
    // only a sound where it stands.
    game.lan_acting = Some(1);
    let pos = [8, 64, 8];
    let pumpkin = game
        .pack
        .assets
        .registry
        .default_state("minecraft:pumpkin")
        .unwrap();
    game.world.set(pos[0], pos[1], pos[2], pumpkin).unwrap();
    let max = game.items().get("minecraft:shears").unwrap().max_damage;
    let mut shears = ItemStack::new("minecraft:shears", 1);
    shears.damage = max - 1;
    game.offhand = Some(shears.clone());
    let stick = ItemStack::new("minecraft:stick", 3);
    game.inventory[game.selected] = Some(stick.clone());
    game.hand = Hand::Off;
    assert!(game.use_pumpkin(pos, [0, 1, 0], Some(&shears)).unwrap());
    // Entity event 48 is OFFHAND's break (47 is the main hand's): the shears
    // that carved wear out, and the main hand keeps its stack.
    assert_eq!(
        game.guest_equipment_breaks.last().copied(),
        Some(48)
    );
    assert!(game.offhand.is_none(), "the shears broke");
    assert_eq!(game.inventory[game.selected], Some(stick));
}
