//! The death of a cart or a boat by command, and a TNT cart's fuse, by network id. An inert level answers as an empty world (false, None).
use super::level::EcsLevel;
use crate::mobs::MobWorld;

impl EcsLevel {
    /// `EcsWorld::kill_vehicle` on the cart or boat `id`: false when no such vehicle is in the level.
    pub fn kill_vehicle(&mut self, mw: &MobWorld<'_>, id: u32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.kill_vehicle(mw, e))
    }

    /// `EcsWorld::cart_fuse` on the cart `id`: None when it is no TNT cart.
    pub fn cart_fuse(&self, id: u32) -> Option<i32> {
        let e = self.entity_of(id)?;
        self.ecs()?.cart_fuse(e)
    }
}
