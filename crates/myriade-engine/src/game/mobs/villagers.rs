//! What villagers do with each other and with the player: gossip and
//! reputation, sharing food and wares, picking up dropped items, making
//! babies, calling iron golems and the prices they ask. What stays here
//! needs the game itself: the particles, the player UUIDs, the merchant menu, the
//! sleep hooks and the wake-up inside a mob pass.
use super::Game;

impl Game<'_> {
    /// `Level.broadcastEntityEvent` of an event whose `handleEntityEvent`
    /// draws particles about the mob (villager 12, 13 and 14, taming 6 and 7,
    /// love 18): the guests get the byte.
    pub(super) fn mob_event_particles(&mut self, id: u32, event: i8) {
        let Some(at) = self.ecs.position(id) else {
            return;
        };
        self.lan_entity_event(id as i32, event, at);
    }
    /// The UUID of a player by connection (None for the host).
    pub(super) fn player_uuid_of(&self, who: Option<u32>) -> Option<[i32; 4]> {
        let uuid = match who {
            None => return self.level.player_uuid,
            // While that guest acts its own UUID is swapped in.
            Some(conn) if self.lan_acting == Some(conn) => self.lan_acting_uuid?,
            Some(conn) => self.lan.as_ref()?.hub.guest(conn)?.uuid,
        };
        Some(myriade_engine::lan::playerdata::uuid_ints(uuid))
    }
    /// The Hero of the Village amplifier of the player, if any.
    pub(crate) fn hero_amplifier(&self) -> Option<i32> {
        self.potions.effects.amplifier("hero_of_the_village")
    }
}

#[cfg(test)]
mod entity_event_particle_tests {
    /// The raid sweat (42) and the dolphin's treasure hearts (38) are in the
    /// table that now decides which entity events draw particles.
    #[test]
    fn the_table_draws_the_raid_sweat_and_the_dolphin_event() {
        let kind = |entity, event| {
            myriade_engine::particles::entity_event_burst(entity, event).map(|(k, _)| k)
        };
        assert_eq!(kind("villager", 42), Some("splash"));
        assert_eq!(kind("dolphin", 38), Some("happy_villager"));
        assert_eq!(kind("villager", 2), None);
    }
}
