//! The players' uuids on the level: the host's connection and departure, passed to the world. Inert on a client.
use super::level::EcsLevel;

impl EcsLevel {
    /// `EcsWorld::player_joined`: nothing on an inert level.
    pub fn player_joined(&mut self, id: u32, uuid: [i32; 4]) {
        if let Some(w) = self.ecs_mut() {
            w.player_joined(id, uuid);
        }
    }

    /// `EcsWorld::player_left`: nothing on an inert level.
    pub fn player_left(&mut self, id: u32) {
        if let Some(w) = self.ecs_mut() {
            w.player_left(id);
        }
    }
}
