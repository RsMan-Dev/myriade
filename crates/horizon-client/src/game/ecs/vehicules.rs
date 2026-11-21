//! The creatures a host vehicle (boat, minecart: still in `Game.boats` and `Game.carts`) meets, read on the ECS by network id. The
//! creatures are found through the spatial index, never by a scan of the entities.
use super::super::Game;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cow::Vel;

impl Game<'_> {
    /// The living creatures within `r` of `at`, from the spatial index (`ids_near`).
    pub(in crate::game) fn ecs_creatures_near(&self, at: [f64; 3], r: f64) -> Vec<u32> {
        self.ecs.ids_near(at, r).into_iter().filter(|&id| self.ecs_living(id).is_some()).collect()
    }
    /// The velocity of the creature `id` (`Vel`).
    pub(in crate::game) fn ecs_mob_velocity(&self, id: u32) -> Option<Vec3> {
        let (w, e) = self.ecs_living(id)?;
        w.get::<Vel>(e).map(|v| v.0)
    }
    /// `Entity.push` on the creature `id`: adds `dv` to its velocity on the ECS; false when it is not in the ECS.
    pub(in crate::game) fn ecs_push_mob(&mut self, id: u32, dv: Vec3) -> bool {
        let Some(v) = self.ecs_mob_velocity(id) else {
            return false;
        };
        let Some(e) = self.ecs.entity_of(id) else {
            return false;
        };
        self.ecs.set_velocity(e, v.add_vector(dv));
        true
    }
}
