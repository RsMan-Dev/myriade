//! The speed and jump factors of the block an entity stands on (`Entity.getBlockSpeedFactor`, `Entity.getBlockJumpFactor`). The table is
//! the level's movement properties (`MobWorld::movement`), read in place; the rule (the block at the feet, else the block below) is
//! horizon-world's `movement_support`. The block factor parts of the host stay on its side. Pure reads: nothing is kept per entity.
use myriade_engine_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;
use horizon_core::position::BlockPos;
use horizon_world::movement_properties::MovementBlock;
use horizon_world::movement_support::{block_jump_factor, block_speed_factor, MovementBlockWorld, MovementLocation};
use bevy_ecs::prelude::*;

/// The entities the block's speed factor slows (`Entity.move`'s last step scales the horizontal velocity by it). Set by the lot of the
/// entity, never by its species.
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct BlockSpeed;

/// The movement block reads of the level, for `movement_support`: the states of `MobWorld::world`, their factors from `MobWorld::movement`.
pub struct PBlocks<'a> {
    pub mw: &'a MobWorld<'static>,
}

impl MovementBlockWorld for PBlocks<'_> {
    type Error = ();
    type State = MovementBlock;
    fn block_state(&mut self, p: BlockPos) -> Result<MovementBlock, ()> {
        let state = self.mw.world.block(p.x, p.y, p.z);
        self.mw.movement.get(state).map_err(|_| ())
    }
    fn is_fence(&mut self, s: MovementBlock) -> Result<bool, ()> {
        Ok(s.fence)
    }
    fn is_wall(&mut self, s: MovementBlock) -> Result<bool, ()> {
        Ok(s.wall)
    }
    fn is_fence_gate(&mut self, s: MovementBlock) -> Result<bool, ()> {
        Ok(s.coefficients.fence_gate)
    }
    fn is_water(&mut self, s: MovementBlock) -> Result<bool, ()> {
        Ok(s.water)
    }
    fn is_bubble_column(&mut self, s: MovementBlock) -> Result<bool, ()> {
        Ok(s.bubble_column)
    }
    fn jump_factor(&mut self, s: MovementBlock) -> Result<f32, ()> {
        Ok(s.coefficients.jump_factor)
    }
    fn speed_factor(&mut self, s: MovementBlock) -> Result<f32, ()> {
        Ok(s.coefficients.speed_factor)
    }
}

/// The location of a point with no supporting block; `None` outside the loaded chunks, where nothing slows.
fn location(mw: &MobWorld<'static>, at: [f64; 3]) -> Option<MovementLocation> {
    let block = BlockPos::new(at[0].floor() as i32, at[1].floor() as i32, at[2].floor() as i32);
    mw.world.is_loaded_at(block.x, block.y, block.z).then(|| MovementLocation {
        position: Vec3::new(at[0], at[1], at[2]),
        block_position: block,
        supporting_block: None,
    })
}

/// The speed factor at `at` (1.0 outside the loaded chunks).
pub fn speed(mw: &MobWorld<'static>, at: [f64; 3]) -> f32 {
    location(mw, at).and_then(|l| block_speed_factor(&mut PBlocks { mw }, l).ok()).unwrap_or(1.0)
}

/// The jump factor at `at` (1.0 outside the loaded chunks).
pub fn jump(mw: &MobWorld<'static>, at: [f64; 3]) -> f32 {
    location(mw, at).and_then(|l| block_jump_factor(&mut PBlocks { mw }, l).ok()).unwrap_or(1.0)
}
