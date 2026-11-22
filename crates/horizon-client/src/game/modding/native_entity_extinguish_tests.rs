//! Observable fire state, sounds and ordered random draws through typed handles.
#![cfg(test)]
use crate::game::native_entity_refs::tests::with_game;
use horizon_client::{
    entities::{ArmorStand, ItemEntity},
    items::ItemStack,
    motion::PlayerMotion,
};
use horizon_core::{
    geometry::Vec3,
    random::{LegacyRandom, RandomSource},
};

// java.util.Random(1234): first floats .6465821266174316, .2597898840904236,
// then .9513576626777649. Java float arithmetic gives pitch bits 0x3fe09a90.
const FIRST: f32 = 0.6465821;
const THIRD: f32 = 0.95135766;
const PITCH: u32 = 0x3fe09a90;

#[test]
#[ignore = "daily"]
fn extinguish_fire_preserves_player_guest_stand_and_item_sound_routes() {
    crate::game::native_guest_turn_tests::with_guests(|game| {
        let at = Vec3::new(3.0, 64.0, 5.0);
        let mut body = PlayerMotion::new(game.motion, at).unwrap();
        body.entity_id = 7;
        game.player = Some(body);
        game.bind_local_player_residency();
        // The joined-guest fixture starts before its lazy motion body exists.
        game.for_player(crate::game::commands::Target::Guest(12), |_| {})
            .unwrap();
        game.armor_stands.push(ArmorStand::new(22, at, 0.0));
        game.dropped.push(ItemEntity::new(
            23,
            ItemStack::new("minecraft:stone", 1),
            at,
            Vec3::ZERO,
            0.0,
        ));
        for (id, source) in [(7, 7), (12, 7), (22, 6), (23, 8)] {
            let entity = game.entity(id).unwrap();
            for ticks in [72, 0, -12] {
                assert!(
                    entity.set_remaining_fire_ticks(game, ticks),
                    "entity {id}, ticks {ticks}"
                );
                *game.particles.random() = LegacyRandom::new(1234);
                game.dropped[0].random = horizon_core::random::EntityRandom::own(LegacyRandom::new(1234));
                game.audio.capture = Some(Vec::new());
                let actor = game.lan_acting;
                assert!(entity.extinguish_fire(game));
                assert_eq!(game.lan_acting, actor);
                assert_eq!(entity.fire_ticks(game), Some(ticks.min(0)));
                let captured = game.audio.capture.as_ref().unwrap();
                if ticks > 0 {
                    assert_eq!(captured.len(), 1);
                    let sound = &captured[0];
                    assert_eq!(sound.event, "minecraft:entity.generic.extinguish_fire");
                    assert_eq!(sound.position, [3.0, 64.0, 5.0]);
                    assert_eq!(sound.source, source);
                    assert_eq!(sound.volume, 0.7);
                    assert_eq!(sound.pitch.to_bits(), PITCH);
                } else {
                    assert!(captured.is_empty());
                }
                assert_eq!(
                    game.particles.random().next_float(),
                    if ticks > 0 && id != 23 { THIRD } else { FIRST }
                );
                assert_eq!(
                    game.dropped[0].random.next_float(),
                    if ticks > 0 && id == 23 { THIRD } else { FIRST }
                );
            }
        }
        let entity = game.entity(23).unwrap();
        let item = &mut game.dropped[0];
        item.stack = ItemStack::new("minecraft:netherite_ingot", 1);
        item.fire_ticks = 72;
        item.random = horizon_core::random::EntityRandom::own(LegacyRandom::new(1234));
        *game.particles.random() = LegacyRandom::new(1234);
        game.audio.capture = Some(Vec::new());
        assert!(entity.extinguish_fire(game));
        assert_eq!(game.dropped[0].fire_ticks, 0);
        assert!(game.audio.capture.as_ref().unwrap().is_empty());
        assert_eq!(game.dropped[0].random.next_float(), FIRST);
        assert_eq!(game.particles.random().next_float(), FIRST);
    });
}
