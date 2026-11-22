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
                .ecs
                .credited_uuid(m.id)
                .map(crate::game::combat::PlayerCredit)
                .and_then(|credit| self.resolve_player_credit(credit))
                .and_then(|conn| self.player_entity_id_by_connection(conn))
                .map_or(0, |id| id as u32),
            None => 0,
        };
        (damage_type, attacker)
    }
}

#[cfg(test)]
#[path = "mob_tick_access_tests.rs"]
mod mob_tick_access_tests;
