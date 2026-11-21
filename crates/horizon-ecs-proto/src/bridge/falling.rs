//! The falling blocks the host drops (a block it breaks, a drop of a mod): the cell is cleared and the falling block starts at its centre,
//! with the item its state drops and the block entity data it lands with.
use crate::bridge::host::EcsWorld;
use crate::cow::Vel;
use crate::falling_block::{self, FallSettings, FallingBlock};
use crate::falling_land;
use bevy_ecs::entity::Entity;
use horizon_client_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;
use horizon_core::nbt::Compound;
use horizon_world::registry::BlockStateId;

impl EcsWorld {
    /// `FallingBlockEntity.fall` of `state` at `at`, carrying `data` to the block it lands as, with `settings` and the initial `speed`.
    pub fn drop_block(&mut self, mw: &MobWorld<'_>, state: BlockStateId, at: [i32; 3], data: Option<Compound>, settings: FallSettings, speed: Vec3) -> Entity {
        self.lent(mw, |w| {
            let drop = falling_land::drop_stack(w, state);
            let e = falling_block::fall(w, at, state, drop);
            if let Some(mut b) = w.get_mut::<FallingBlock>(e) {
                b.data = data;
                b.settings = settings;
            }
            if let Some(mut v) = w.get_mut::<Vel>(e) {
                v.0 = speed;
            }
            e
        })
    }
}
