//! `falling_tick`: a falling block's parts hear each tick it ends still in the air, once the
//! ordered pass that ticked it is over (the falling block is lent out during its own tick).
use super::Game;
use horizon_core::position::BlockPos;
use horizon_world::registry::BlockStateId;

impl Game<'_> {
    /// `HostEvent::FallingTick` of P's falling block `id` (still in the air at `at`): the parts of its state hear it, as a host one does.
    pub(in crate::game) fn ecs_falling_tick(&mut self, id: u32, state: BlockStateId, at: [f64; 3]) {
        let Some(epoch) = self.native_lifecycle.epoch() else { return };
        let pos = BlockPos::new(at[0].floor() as i32, at[1].floor() as i32, at[2].floor() as i32);
        self.native_falling_tick(state, pos, mod_api::EntityRef::new(epoch, id));
    }
}
