//! The acting player as its server names it.
use crate::game::Game;

impl Game<'_> {
    /// The local player's UUID, as servers know it.
    pub(in crate::game) fn own_uuid(&self) -> u128 {
        self.level.player_uuid.map_or(0, myriade_engine::lan::playerdata::ints_uuid)
    }
    /// The entity id the local player has on its server: the acting player's own.
    pub(in crate::game) fn own_entity_id(&self) -> i32 {
        self.player.as_ref().map_or(0, |p| p.entity_id)
    }
}
