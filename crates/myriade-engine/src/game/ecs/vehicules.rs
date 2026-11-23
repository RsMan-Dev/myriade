//! The creatures a ECS vehicle (boat, minecart) meets, read by network id. The
//! creatures are found through the spatial index, never by a scan of the entities.
use super::super::Game;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cow::Vel;

impl Game<'_> {
    /// The velocity of the creature `id` (`Vel`).
    pub(in crate::game) fn ecs_mob_velocity(&self, id: u32) -> Option<Vec3> {
        let (w, e) = self.ecs_living(id)?;
        w.get::<Vel>(e).map(|v| v.0)
    }
}
