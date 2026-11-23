//! The vehicles' pushes by network id (P `bridge/vehicle_push.rs`): the push of an entity, the entities a vehicle touches, its box and whether
//! a push moves it. An inert level answers as an empty world.
use super::level::EcsLevel;

impl EcsLevel {
    /// Adds `impulse` to the velocity of the entity `id` and wakes it (`EcsWorld::push_entity`).
    pub fn push_entity(&mut self, id: u32, impulse: [f64; 3]) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(w) = self.ecs_mut() {
            w.push_entity(e, impulse);
        }
    }
    /// The ids of the linked entities whose box `vehicle` touches; an entity with no id yet is left out.
    pub fn vehicle_contacts(&self, vehicle: u32) -> Vec<u32> {
        let (Some(v), Some(w)) = (self.entity_of(vehicle), self.ecs()) else { return Vec::new() };
        w.vehicle_contacts(v).into_iter().filter_map(|e| w.id_of(e)).collect()
    }
    /// The box of the entity `id` as its minimum and maximum corners.
    pub fn push_bounds(&self, id: u32) -> Option<([f64; 3], [f64; 3])> {
        let b = self.ecs()?.bounds(self.entity_of(id)?)?;
        let (mn, mx) = (b.min_position(), b.max_position());
        Some(([mn.x, mn.y, mn.z], [mx.x, mx.y, mx.z]))
    }
    /// Whether a push moves the entity `id` (false when it is not in the level).
    pub fn push_moves(&self, id: u32) -> bool {
        let (Some(e), Some(w)) = (self.entity_of(id), self.ecs()) else { return false };
        w.push_moves(e)
    }
    /// The boxes of the solid entities the sweep of the entity `id` (its `bounds` moved by `movement`) reaches (P `EcsWorld::solid_colliders`).
    pub fn solid_colliders(&self, id: u32, bounds: horizon_core::geometry::Aabb, movement: [f64; 3]) -> Vec<horizon_core::geometry::Aabb> {
        let (Some(e), Some(w)) = (self.entity_of(id), self.ecs()) else { return Vec::new() };
        w.solid_colliders(e, bounds, movement)
    }
    /// The boxes of the solid entities in `sweep` for a mover the level does not hold: a player (`player_y` is its feet, `riding` its
    /// vehicle) or, with no `player_y`, any other (P `EcsWorld::solid_boxes`).
    pub fn solid_boxes(&self, sweep: horizon_core::geometry::Aabb, player_y: Option<f64>, riding: Option<u32>) -> Vec<horizon_core::geometry::Aabb> {
        use horizon_ecs_proto::world_push::Mover;
        let Some(w) = self.ecs() else { return Vec::new() };
        let mover = match player_y {
            Some(y) => Mover::Player { y, vehicle: riding.and_then(|v| self.entity_of(v)) },
            None => Mover::Other,
        };
        w.solid_boxes(sweep, mover)
    }
    /// Whether the entity `id` is a boat or a cart (false when it is not in the level).
    pub fn is_vehicle(&self, id: u32) -> bool {
        let (Some(e), Some(w)) = (self.entity_of(id), self.ecs()) else { return false };
        w.is_vehicle(e)
    }
    /// One moving piston's push on the ECS creatures its moved block meets (P `EcsWorld::piston_push`); an inert level does nothing.
    pub fn piston_push(
        &mut self,
        mw: &crate::mobs::MobWorld<'_>,
        pos: horizon_core::position::BlockPos,
        piston: horizon_world::piston_shape::MovingPistonState,
        new_progress: f32,
        movement: horizon_core::geometry::Direction,
        block: &horizon_ecs_proto::world_push::PushBlock<'_>,
        gate: Option<horizon_ecs_proto::world_push::PushGate<'_>>,
    ) {
        if let Some(w) = self.ecs_mut() {
            w.piston_push(mw, pos, piston, new_progress, movement, block, gate);
        }
    }
    /// The ids of the boats and carts whose BOX meets `min..=max` (`getEntities(box)`; `boats_in`/`carts_in` test the position).
    pub fn vehicles_touching(&self, min: horizon_core::geometry::Vec3, max: horizon_core::geometry::Vec3) -> Vec<u32> {
        self.ecs().map_or_else(Vec::new, |ecs| ecs.vehicles_touching(min, max).into_iter().filter_map(|e| ecs.id_of(e)).collect())
    }
    /// The ids of the living creatures alive whose box meets `min..=max` (what a projectile can hit).
    pub fn living_touching(&self, min: horizon_core::geometry::Vec3, max: horizon_core::geometry::Vec3) -> Vec<u32> {
        self.ecs().map_or_else(Vec::new, |ecs| ecs.living_touching(min, max).into_iter().filter_map(|e| ecs.id_of(e)).collect())
    }
}
