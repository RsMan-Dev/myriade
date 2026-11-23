//! A living entity's absorption amount for native server mods, through a
//! `LivingRef` (docs/modding/src/how-to/native-mods.md, "Entity and level
//! handles"): `LivingEntity.getAbsorptionAmount` and `setAbsorptionAmount`.
//! The read answers on both sides for the local player; the write is
//! server only, journaled like `living_set_health`.
use super::Game;
use super::commands::Target;
use mod_api::LivingRef;

impl Game<'_> {
    /// The entity's absorption amount (`LivingEntity.getAbsorptionAmount`).
    /// During a player's turn only that player resolves, as for health. None
    /// for a stale handle, an armor stand or the dragon (they carry none).
    pub(super) fn living_absorption(&self, living: LivingRef) -> Option<f32> {
        match self.native_living_read_target(living)? {
            Target::Player => Some(self.player.as_ref()?.health.absorption()),
            Target::Guest(conn) => {
                let guest = self.command_guest(conn)?;
                Some(
                    guest
                        .motion
                        .as_ref()
                        .map_or(guest.saved_absorption.unwrap_or(0.0), |m| {
                            m.health.absorption()
                        }),
                )
            }
            Target::Mob(id) if self.ecs.is_active() => self.ecs.absorption(id),
            _ => None,
        }
    }

    /// Sets the entity's absorption amount (`LivingEntity.setAbsorptionAmount`),
    /// cut to `0..=max_absorption`. No heal or damage hook runs. A player's new
    /// amount reaches their client and the trackers through the synced data.
    /// False for a stale handle, a non-finite amount, an armor stand or the
    /// dragon.
    pub(super) fn living_set_absorption(&mut self, living: LivingRef, amount: f32) -> bool {
        if !amount.is_finite() {
            return false;
        }
        match self.native_living_target(living) {
            Some(t @ (Target::Player | Target::Guest(_))) => self
                .for_player(t, |g| {
                    g.player.as_ref()?;
                    g.journal_player_health();
                    g.player.as_mut()?.set_health(amount, true).ok()
                })
                .flatten()
                .is_some(),
            Some(Target::Mob(id)) if self.ecs.is_active() => {
                self.journal_mob_health(id);
                self.ecs.set_absorption(id, amount)
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::native_guest_turn_tests::with_acting_guest;
    use horizon_core::geometry::Vec3;

    #[test]
    #[ignore = "daily"]
    fn a_players_absorption_is_cut_to_zero_and_max_absorption_and_synced() {
        with_acting_guest(Vec3::ZERO, |game| {
            game.player
                .as_mut()
                .unwrap()
                .set_modifier(
                    "minecraft:max_absorption",
                    "test:absorption",
                    10.0,
                    "add_value",
                    true,
                )
                .unwrap();
            let living = game.living(game.entity(12).unwrap()).unwrap();
            assert_eq!(living.absorption(game), Some(0.0));
            let events = game.player.as_ref().unwrap().events.len();
            assert!(living.set_absorption(game, 8.0));
            assert_eq!(living.absorption(game), Some(8.0));
            assert_eq!(game.player.as_ref().unwrap().health.absorption(), 8.0);
            assert!(
                game.player.as_ref().unwrap().events.len() > events,
                "the new amount is queued for the client and the trackers"
            );
            assert!(living.set_absorption(game, 20.0));
            assert_eq!(living.absorption(game), Some(10.0));
            assert!(living.set_absorption(game, -1.0));
            assert_eq!(living.absorption(game), Some(0.0));
            assert!(living.set_absorption(game, 6.0));
            assert!(!living.set_absorption(game, f32::NAN));
            assert!(!living.set_absorption(game, f32::INFINITY));
            assert_eq!(living.absorption(game), Some(6.0));
            assert_eq!(game.player.as_ref().unwrap().health.absorption(), 6.0);
        });
    }

}
