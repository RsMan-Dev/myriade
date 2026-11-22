//! Resumable borrowed mob-tick region; the caller owns final restoration.
use super::*;

impl Game<'_> {
    /// The killing blow of a mob as the death hooks name it: its damage
    /// type and the causing entity's id (0 for none).
    pub(in crate::game) fn mob_damage_source(&self, m: &Mob) -> (&'static str, u32) {
        let (damage_type, attacker) = match m.last_damage {
            Some((d, _)) => (source_damage_type(d), d.attacker),
            None => ("minecraft:generic", None),
        };
        let attacker = match attacker {
            Some(Who::Mob(id)) => id,
            Some(Who::Player) => self
                .last_hurt_by_player
                .get(&m.id)
                .and_then(|credit| self.resolve_player_credit(*credit))
                .and_then(|conn| self.player_entity_id_by_connection(conn))
                .map_or(0, |id| id as u32),
            None => 0,
        };
        (damage_type, attacker)
    }
}

#[cfg(test)]
mod hoglin_routing_tests {
    use super::*;

    #[test]
    fn broadcast_keeps_source_player_connection() {
        for (seen_guest, tag, expected) in [
            (Some(12), None, Some(12)),
            (None, None, None),
            (Some(12), Some(Some(13)), Some(13)),
            (Some(12), Some(None), None),
        ] {
            let event = MobEvent::HoglinAttackTarget {
                mob: 9001,
                target: Who::Player,
                player: None,
            };
            let mut events = vec![match tag {
                Some(conn) => MobEvent::ForPlayer {
                    conn,
                    event: Box::new(event),
                },
                None => event,
            }];
            route_player_events(&mut events, 0, seen_guest, &[], &mut Vec::new(), 9000);
            assert!(matches!(events[0], MobEvent::HoglinAttackTarget {
                player, ..
            } if player == expected));
        }
    }
}

#[cfg(test)]
#[path = "mob_tick_access_tests.rs"]
mod mob_tick_access_tests;
