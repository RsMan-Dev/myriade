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
    pub fn pushable(&self, id: u32) -> bool {
        let (Some(e), Some(w)) = (self.entity_of(id), self.ecs()) else { return false };
        w.pushable(e)
    }
    /// Whether the entity `id` is a boat or a cart (false when it is not in the level).
    pub fn is_vehicle(&self, id: u32) -> bool {
        let (Some(e), Some(w)) = (self.entity_of(id), self.ecs()) else { return false };
        w.is_vehicle(e)
    }
    /// The ids of the boats and carts whose BOX meets `min..=max` (`getEntities(box)`; `boats_in`/`carts_in` test the position).
    pub fn vehicles_touching(&self, min: horizon_core::geometry::Vec3, max: horizon_core::geometry::Vec3) -> Vec<u32> {
        self.ecs().map_or_else(Vec::new, |ecs| ecs.vehicles_touching(min, max).into_iter().filter_map(|e| ecs.id_of(e)).collect())
    }
}
