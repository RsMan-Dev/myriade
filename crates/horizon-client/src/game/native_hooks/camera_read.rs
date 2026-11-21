use super::*;

/// Helpers for native mods, kept as stable as the hook points.
// Native mods use these; nothing in Horizon does.
#[allow(dead_code)]
impl Game<'_> {
    /// `Minecraft.setCameraEntity`: the view follows the local player or a mob the server shows
    /// this window. False, with the camera untouched, for anything else.
    pub(in crate::game) fn set_camera_entity(&mut self, entity: EntityRef) -> bool {
        use crate::game::commands::Target;
        let id = match self.native_client_entity_target(entity) {
            Some(Target::Player | Target::Mob(_)) => entity.id() as i32,
            _ => return false,
        };
        self.remote_set_camera(id);
        true
    }
    /// Collision boxes in block-local coordinates with an empty collision context.
    pub(in crate::game) fn block_collision(&self, pos: [i32; 3]) -> Vec<[f64; 6]> {
        self.native_block_collision_in(&self.world, pos)
    }
}
