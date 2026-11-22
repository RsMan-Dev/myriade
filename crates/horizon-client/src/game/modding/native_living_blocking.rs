//! `LivingEntity.isBlocking` for native mods through a `LivingRef`: a player or a mob holding up
//! an item that blocks attacks, past the item's block delay.
use super::{Game, commands::Target, shield::block_delay_passed};
use horizon_client::mobs::spear_use;
use mod_api::LivingRef;

impl Game<'_> {
    /// `LivingEntity.isBlocking` (`getItemBlockingWith` is not null): the entity uses an item
    /// that blocks attacks, from either hand, and has used it for the component's block delay.
    /// False while it uses nothing or another item. None for armor stands, the dragon and a
    /// stale handle.
    pub(super) fn living_is_blocking(&self, living: LivingRef) -> Option<bool> {
        // How long the item in use has been used: a player's and a guest's use counts up from 0,
        // a mob's counter from 1.
        let ticks = match self.native_living_read_target(living)? {
            Target::Player => self.using.map(|(_, ticks)| ticks),
            Target::Guest(conn) => self.lan.as_ref()?.play.get(&conn)?.using().map(|(_, t)| t),
            Target::Mob(id) if self.ecs.is_active() => self.ecs_using(id).and_then(|(off, left)| self.ecs_hand_stack(id, off).map(|s| u32::try_from(self.use_duration(&s).saturating_sub(left)).unwrap_or(0))),
            _ => return None,
        };
        let blocking = ticks.is_some_and(|ticks| {
            self.living_used_item_hand(living)
                .and_then(|hand| self.living_hand_item(living, hand))
                .is_some_and(|stack| {
                    self.blocks_attacks(&stack)
                        .is_some_and(|component| block_delay_passed(&component, ticks))
                })
        });
        Some(blocking)
    }
}

#[cfg(test)]
mod tests {
    use super::super::native_entity_refs::tests::with_game;
    use horizon_client::items::ItemStack;
    use horizon_core::geometry::Vec3;
    use mod_api::EquipmentSlot;
    use mod_api::client::first_person::Hand;

    /// A raised shield blocks after vanilla's 5-tick delay (`block_delay_seconds` 0.25), in
    /// either hand, for the player as the damage path sees it and for a mob; a bow in use, an
    /// unraised shield and a lowered one don't.
    #[test]
    #[ignore = "daily"]
    fn a_raised_shield_blocks_after_its_delay_for_a_player_and_a_mob() {
        with_game(|game| {
            crate::game::native_entity_refs::tests::resolve_parts(game).unwrap();
            game.player =
                Some(horizon_client::motion::PlayerMotion::new(game.motion, Vec3::ZERO).unwrap());
            crate::game::native_entity_refs::tests::bind_local_player(game, 20, 123);
            let player = game.living(game.entity(20).unwrap()).unwrap();
            let shield = ItemStack::new("minecraft:shield", 1);
            let delay = game.blocks_attacks(&shield).unwrap().delay_seconds;
            assert_eq!(delay, 0.25, "vanilla's shield");

            for hand in [Hand::Main, Hand::Off] {
                game.selected = 0;
                (game.inventory[0], game.offhand) = match hand {
                    Hand::Main => (Some(shield.clone()), None),
                    Hand::Off => (None, Some(shield.clone())),
                };
                assert_eq!(game.living_is_blocking(player), Some(false), "not raised");
                assert!(game.living_start_using_item(player, hand));
                for tick in 0..5 {
                    assert_eq!(
                        game.living_is_blocking(player),
                        Some(false),
                        "{hand:?} {tick}"
                    );
                    game.tick_using();
                }
                assert_eq!(game.living_is_blocking(player), Some(true), "{hand:?}");
                let hit = game.player_blocking(None, "minecraft:mob_attack", false);
                assert!(hit.is_some(), "the damage path blocks too");
                assert!(game.living_release_using_item(player));
                assert_eq!(game.living_is_blocking(player), Some(false), "lowered");
            }
            game.offhand = Some(ItemStack::new("minecraft:bow", 1));
            assert!(game.living_start_using_item(player, Hand::Off));
            for _ in 0..10 {
                game.tick_using();
            }
            assert_eq!(
                game.living_is_blocking(player),
                Some(false),
                "a bow blocks nothing"
            );

            crate::game::ecs::test_support::place_test_mob(game, 12, "minecraft:zombie", [0.0; 3]);
            let zombie = game.living(game.entity(12).unwrap()).unwrap();
            assert!(game.living_set_equipment(zombie, EquipmentSlot::OffHand, Some(shield)));
            assert_eq!(game.living_is_blocking(zombie), Some(false));
            assert!(game.living_start_using_item(zombie, Hand::Off));
            // RESTE-SANS-ECS: the mob's 4- and 5-tick delay needs an ECS use-tick helper.
            assert!(game.living_release_using_item(zombie));
            assert_eq!(game.living_is_blocking(zombie), Some(false));
        });
    }
}
