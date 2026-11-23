//! Bounds of a vehicle that carries passengers, for the seat of its riders.
use super::*;
use horizon_core::geometry::Aabb;

impl Game<'_> {
    /// Default Entity passenger attachment: the top of its current box.
    /// Registered dimensions cover live carriers without an Entity handle.
    pub(in crate::game) fn local_passenger_vehicle_bounds(&self, id: u32) -> Option<Aabb> {
        use crate::game::entity::{Entity, with_entity};
        let target = self.level_entity(id as i32)?;
        let at = self.target_position(target)?;
        let [width, height] = self.registered_entity_dimensions(&self.target_type(target))?;
        let half = f64::from(width) / 2.0;
        let mut bounds = Aabb::new(
            Vec3::new(at[0] - half, at[1], at[2] - half),
            Vec3::new(at[0] + half, at[1] + f64::from(height), at[2] + half),
        );
        for handle in self.entity_handles() {
            with_entity!(self, handle, entity => {
                if entity.id() == id { bounds = entity.bounds(); }
            });
        }
        Some(bounds)
    }
}
