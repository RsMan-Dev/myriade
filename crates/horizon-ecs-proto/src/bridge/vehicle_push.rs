//! The vehicles' pushes through the bridge (`vehicle_push.rs`): the push of an entity, the entities a vehicle touches, whether a push moves an
//! entity, and the bumps the pushes wrote since the last drain.
use crate::bridge::host::EcsWorld;
use crate::rules::push_rules::TOUCH_REACH;
use crate::vehicle_push;
use bevy_ecs::entity::Entity;

impl EcsWorld {
    /// Adds `impulse` to the velocity of `e` and wakes it.
    pub fn push_entity(&mut self, e: Entity, impulse: [f64; 3]) {
        vehicle_push::push(self.world_mut(), e, impulse);
    }
    /// The entities whose box a vehicle touches (the index answers; nothing is scanned).
    pub fn vehicle_contacts(&self, vehicle: Entity) -> Vec<Entity> {
        let mut out = Vec::new();
        vehicle_push::contacts(self.world(), vehicle, TOUCH_REACH, &mut out);
        out
    }
    /// Whether `e` is a boat or a cart (the blast's vehicle pass is P's).
    pub fn is_vehicle(&self, e: Entity) -> bool {
        let w = self.world();
        w.get::<crate::boat_state::BoatMarker>(e).is_some() || w.get::<crate::cart_state::Cart>(e).is_some()
    }
    /// Whether a push moves `e` (`Entity.isPushable`, not carrying riders).
    pub fn push_moves(&self, e: Entity) -> bool {
        vehicle_push::pushable(self.world(), e)
    }
    /// `EntityGetter.getEntityCollisions` for `e` sweeping `bounds` by `movement`: the boxes of the solid entities (a boat, a living
    /// shulker, an adult happy ghast on its still timeout) its sweep reaches, from the index; empty for a boat or a cart.
    pub fn solid_colliders(&self, e: Entity, bounds: horizon_core::geometry::Aabb, movement: [f64; 3]) -> Vec<horizon_core::geometry::Aabb> {
        let m = horizon_core::geometry::Vec3::new(movement[0], movement[1], movement[2]);
        crate::world_push::solid_colliders(self.world(), e, bounds, m)
    }
    /// `solid_colliders` for a mover the level does not hold (a player, an item, a stand), over `sweep`.
    pub fn solid_boxes(&self, sweep: horizon_core::geometry::Aabb, mover: crate::world_push::Mover) -> Vec<horizon_core::geometry::Aabb> {
        crate::world_push::solid_boxes(self.world(), sweep, mover)
    }
    /// The push of the moving piston at `pos` (progress `piston.progress` to `new_progress` along `movement`) on the ECS creatures
    /// its moved block meets, once for the whole block (`world_push::piston_push`); the host keeps the players.
    pub fn piston_push(
        &mut self,
        mw: &myriade_engine_lib::mobs::MobWorld<'_>,
        pos: horizon_core::position::BlockPos,
        piston: horizon_world::piston_shape::MovingPistonState,
        new_progress: f32,
        movement: horizon_core::geometry::Direction,
        block: &crate::world_push::PushBlock<'_>,
        gate: Option<crate::world_push::PushGate<'_>>,
    ) {
        self.lent(mw, |w| crate::world_push::piston_push(w, pos, piston, new_progress, movement, block, gate));
    }
}
