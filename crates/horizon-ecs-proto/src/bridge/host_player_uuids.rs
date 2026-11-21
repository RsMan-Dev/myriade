//! The players' uuids on the bridge: the host tells a connection and a departure, and reads the uuid of a player id (`player_uuids`).
use crate::bridge::host::EcsWorld;
use crate::bridge::uuids::{from_words, to_words};
use crate::player_uuids;

impl EcsWorld {
    /// The player `id` joined with `uuid` (its four words): the owners and leads waiting for it are set.
    pub fn player_joined(&mut self, id: u32, uuid: [i32; 4]) {
        player_uuids::join(self.world_mut(), id, from_words(uuid));
        super::leash_save::resolve_pending(self.world_mut());
    }

    /// The player `id` left; its uuid is kept (see `player_uuids::leave`).
    pub fn player_left(&mut self, id: u32) {
        player_uuids::leave(self.world_mut(), id);
    }

    /// The uuid of player `id` as the save writes it; None when it never joined.
    pub fn player_uuid(&self, id: u32) -> Option<[i32; 4]> {
        player_uuids::uuid_of(self.world(), id).map(to_words)
    }
}
