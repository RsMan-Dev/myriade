//! The ECS entities' ids as the game names them, and the link of a converted entity.
use crate::game::Game;
use bevy_ecs::entity::Entity;

impl Game<'_> {
    /// The kind of the ECS entity `id` (`minecraft:cow`); None when it has no view.
    pub(in crate::game) fn ecs_kind(&self, id: u32) -> Option<String> {
        self.ecs.kind(id).map(|kind| {
            if kind.contains(':') {
                kind.to_owned()
            } else {
                format!("minecraft:{kind}")
            }
        })
    }
    /// The ECS entity `old` turned into `new`: `new` is linked and its id returned, 0 when `old`
    /// has no kind. The caller despawns `old` after this call.
    pub(in crate::game) fn ecs_converted(&mut self, old: u32, new: Entity) -> u32 {
        if self.ecs_kind(old).is_none() {
            return 0;
        }
        self.ecs_link(new)
    }
}
