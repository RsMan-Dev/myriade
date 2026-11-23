//! Enderman hurt teleports and leave-block attempts. Both read state the ECS
//! bridge does not expose yet, so the host side does nothing for them.
use super::Game;
use myriade_engine::Result;
use horizon_core::geometry::Aabb;
use horizon_world::registry::BlockStateId;

impl Game<'_> {
    /// `Enderman.hurtServer` teleports inside the hurt, and `Shulker.hurtServer`
    /// teleports somewhere. Their pending tries live in the mob's own state.
    /// RESTE-SANS-ECS: the pending tries (`counter2`) and the shulker flags have no
    /// ECS read; `repeated_teleport` (`ecs_bridge/level_combat.rs`) is not driven here.
    pub(super) fn settle_hurt_teleports(&mut self) -> Result<()> {
        Ok(())
    }
    /// `EndermanLeaveBlockGoal.tick`: normalize, check, place, post
    /// BLOCK_PLACE with the enderman and the placed state, then clear.
    /// RESTE-SANS-ECS: the carried block (`extra.carried`) has no ECS read or
    /// clear, so the placement is not run from the host side.
    pub(super) fn enderman_place_block(
        &mut self,
        _mob: u32,
        _pos: [i32; 3],
        _carried: BlockStateId,
        _occupants: &[Aabb],
    ) -> Result<()> {
        Ok(())
    }
}
