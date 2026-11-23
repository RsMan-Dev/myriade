//! The ender dragons on the host. The ECS holds each one (`EnderDragon` with its flight, phases and parts); the host reads it through
//! these views and finds it by its network id.
use super::*;
use horizon_ecs_proto::bridge::dragon::DragonView;

impl Game<'_> {
    /// Every dragon of the level with its network id, in entity order.
    pub(in crate::game) fn dragon_views(&self) -> Vec<(u32, DragonView)> {
        self.ecs.dragon_views()
    }
}
