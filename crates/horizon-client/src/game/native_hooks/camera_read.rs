use super::*;

/// Helpers for native mods, kept as stable as the hook points.
// Native mods use these; nothing in Horizon does.
#[allow(dead_code)]
impl Game<'_> {
    /// Collision boxes in block-local coordinates with an empty collision context.
    pub(in crate::game) fn block_collision(&self, pos: [i32; 3]) -> Vec<[f64; 6]> {
        self.native_block_collision_in(&self.world, pos)
    }
}
