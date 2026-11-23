//! `Entity.getBlockBounciness` for the player: the block's restitution after its parts.
use super::{MotionWorld, Result};
use horizon_world::registry::BlockStateId;

impl MotionWorld<'_, '_, '_> {
    /// `Block.getBounceRestitution` of `state` for the player landing on it at
    /// `Entity.getOnPosLegacy`: the block's own coefficient, with no part to change it.
    pub(super) fn part_bounce_restitution(&mut self, state: BlockStateId) -> Result<f32> {
        Ok(self.resources.blocks.get(state)?.coefficients.bounce_restitution)
    }
}
