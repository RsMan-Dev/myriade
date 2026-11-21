//! A player's collision exclusions for mods (arismoko/horizon#3717): boxes of its own level in which that level's
//! blocks don't stop it, as a portal in a wall needs. The server keeps them on the guest, and its
//! body takes them at each turn; a client keeps its own player's on the body it moves. Neither
//! side sends its set to the other: a mod sets both.
use super::{Game, commands::Target};
use horizon_world::collision_exclusions::Refused;
use mod_api::{CollisionExclusionsError, EntityRef};

fn refused(why: Refused) -> CollisionExclusionsError {
    match why {
        Refused::TooMany => CollisionExclusionsError::TooMany,
        Refused::NotFinite => CollisionExclusionsError::NotFinite,
        Refused::Inverted => CollisionExclusionsError::Inverted,
    }
}

impl Game<'_> {
    /// For a server mod: the player's collision exclusions, in place of the last ones (the
    /// guest's, which its moves are checked against, and its body's). Refused for a stale handle,
    /// an entity that isn't a player, and boxes the set refuses; the set there was stays.
    pub(super) fn entity_set_collision_exclusions(
        &mut self,
        entity: EntityRef,
        boxes: &[[f64; 6]],
    ) -> Result<(), CollisionExclusionsError> {
        let conn = match self
            .native_entity_target(entity)
            .ok_or(CollisionExclusionsError::Stale)?
        {
            Target::Guest(conn) => conn,
            Target::Player => self.lan_acting.ok_or(CollisionExclusionsError::Stale)?,
            _ => return Err(CollisionExclusionsError::NotAPlayer),
        };
        // During the guest's own turn its body is the game's player.
        let acting = self.lan_acting == Some(conn);
        let host = self.lan.as_mut().ok_or(CollisionExclusionsError::Stale)?;
        let turn_body = host
            .hub
            .update_guest(conn, |g| {
                g.collision_exclusions.set(boxes)?;
                if let Some(body) = g.motion.as_deref_mut() {
                    body.collision_exclusions
                        .clone_from(&g.collision_exclusions);
                }
                Ok::<_, Refused>(acting.then(|| g.collision_exclusions.clone()))
            })
            .ok_or(CollisionExclusionsError::Stale)?
            .map_err(refused)?;
        if let (Some(set), Some(body)) = (turn_body, self.player.as_mut()) {
            body.collision_exclusions = set;
        }
        Ok(())
    }

    /// For a client mod: the local player's collision exclusions, in place of the last ones.
    /// Refused with no player of its own, and for boxes the set refuses; the set there was stays.
    pub(super) fn set_collision_exclusions(
        &mut self,
        boxes: &[[f64; 6]],
    ) -> Result<(), CollisionExclusionsError> {
        let body = self
            .player
            .as_mut()
            .ok_or(CollisionExclusionsError::Stale)?;
        body.collision_exclusions.set(boxes).map_err(refused)
    }
}

#[cfg(test)]
mod tests {
    use crate::game::Game;
    use mod_api::CollisionExclusionsError as Refusal;

    /// A portal's opening, 1 wide and 2 tall.
    const OPENING: [[f64; 6]; 1] = [[2.0, 64.0, 5.0, 3.0, 66.0, 6.0]];
    const TOO_MANY: [[f64; 6]; 65] = [[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]; 65];

    fn guest_boxes(game: &Game<'_>, conn: u32) -> usize {
        let host = game.lan.as_ref().unwrap();
        host.hub
            .guest(conn)
            .unwrap()
            .collision_exclusions
            .boxes()
            .len()
    }

    #[test]
    #[ignore = "daily"]
    fn a_server_mod_sets_a_guests_exclusions_and_a_refused_set_keeps_them() {
        crate::game::native_guest_turn_tests::with_guests(|game| {
            let entity = game.player(12).unwrap().entity();
            assert_eq!(entity.set_collision_exclusions(game, &OPENING), Ok(()));
            assert_eq!(guest_boxes(game, 12), 1);
            assert_eq!(guest_boxes(game, 13), 0, "only that player's");
            let nan = [[0.0, 64.0, 0.0, 1.0, f64::NAN, 1.0]];
            let inverted = [[3.0, 64.0, 5.0, 2.0, 66.0, 6.0]];
            for (boxes, why) in [
                (&TOO_MANY[..], Refusal::TooMany),
                (&nan[..], Refusal::NotFinite),
                (&inverted[..], Refusal::Inverted),
            ] {
                assert_eq!(entity.set_collision_exclusions(game, boxes), Err(why));
                assert_eq!(guest_boxes(game, 12), 1, "{why:?} keeps the opening");
            }
            // The guest's body takes them for its turn, and a set made during the turn is the
            // body's at once.
            let in_turn = game.as_guest(12, |game| {
                let before = game
                    .player
                    .as_ref()
                    .unwrap()
                    .collision_exclusions
                    .boxes()
                    .len();
                assert_eq!(entity.set_collision_exclusions(game, &[]), Ok(()));
                let after = game
                    .player
                    .as_ref()
                    .unwrap()
                    .collision_exclusions
                    .boxes()
                    .len();
                (before, after)
            });
            assert_eq!(in_turn, Some((1, 0)));
            assert_eq!(guest_boxes(game, 12), 0);
            // A guest that left is gone with its boxes: its handle names nothing.
            assert_eq!(entity.set_collision_exclusions(game, &OPENING), Ok(()));
            let i = game.guest_index(12).unwrap();
            game.lan.as_mut().unwrap().hub.guests.remove(i);
            assert_eq!(
                entity.set_collision_exclusions(game, &OPENING),
                Err(Refusal::Stale)
            );
        });
    }
}
