//! While a piston moves them, blocks are `minecraft:moving_piston` block entities; the
//! sections they sit in are read for their mesh state.
use super::Game;

impl Game<'_> {
    /// The section holding the block at `p` (`SectionPos.blockToSection`).
    pub(super) fn section_of(&self, p: [i32; 3]) -> Option<(i32, i32, i32)> {
        let y = p[1].checked_sub(self.world.layout().min_y())?;
        Some((p[0] >> 4, y.div_euclid(16), p[2] >> 4))
    }
    /// Whether the chunk mesh doesn't show the blocks of a section yet (edited,
    /// meshing, or waiting for its upload).
    pub(super) fn section_mesh_pending_in(&self, section: (i32, i32, i32)) -> bool {
        self.world.dirty_sections.contains(&section)
    }
}
