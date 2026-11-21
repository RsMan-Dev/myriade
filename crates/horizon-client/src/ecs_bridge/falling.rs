//! The falling blocks the host drops, by network id: `EcsWorld::drop_block` on the level, the id linked as any entity's.
use super::level::EcsLevel;
use horizon_client_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;
use horizon_core::nbt::Compound;
use horizon_ecs_proto::falling_block::FallSettings;
use horizon_world::registry::BlockStateId;

impl EcsLevel {
    /// `EcsWorld::drop_block`: the network id of the falling block, linked by `alloc`; 0 when inert.
    pub fn drop_block(&mut self, mw: &MobWorld<'_>, state: BlockStateId, at: [i32; 3], data: Option<Compound>, settings: FallSettings, speed: Vec3, alloc: &mut dyn FnMut() -> u32) -> u32 {
        let Some(w) = self.ecs_mut() else { return 0 };
        let e = w.drop_block(mw, state, at, data, settings, speed);
        self.id_for(e, alloc)
    }
}
