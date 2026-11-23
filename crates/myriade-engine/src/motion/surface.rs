//! What the block an entity stands on or in gives it, as the block's parts answer: its friction
//! (`IBlockExtension.getFriction`), its jump and speed factors, and whether it is a ladder.
use crate::{blocks::BlockFacts, world::World};
use horizon_core::position::BlockPos;
use horizon_world::{
    movement_properties::BlockFactorEdits, native_load_context::LoopContext, registry::BlockStateId,
};

/// The friction of the block at `pos` for the entity `entity` of `kind` on it: the block's own
/// `StateFacts` value, through [`native_friction`].
pub fn block_friction(
    context: &LoopContext,
    entity: u32,
    kind: &str,
    world: &World,
    facts: &BlockFacts,
    pos: BlockPos,
) -> f32 {
    let state = world.block(pos.x, pos.y, pos.z);
    let friction = facts.get(state).friction;
    friction
}

/// The factors the `block_jump_factor` and `block_speed_factor` parts give the blocks an entity
/// reads, for the loop of the side that ticks it.
pub(crate) struct ModBlockFactors<'a>(pub &'a LoopContext);
impl BlockFactorEdits for ModBlockFactors<'_> {
    fn jump(&self, state: BlockStateId, factor: f32) -> f32 {
        factor
    }
    fn speed(&self, state: BlockStateId, factor: f32) -> f32 {
        factor
    }
}
