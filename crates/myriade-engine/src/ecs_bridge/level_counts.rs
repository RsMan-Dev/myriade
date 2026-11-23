//! The counts of the level's ECS world, for the gauges, the reports and the dimension-transfer check: each is one read of
//! `bridge::counts`. An inert level counts nothing.
use super::level::EcsLevel;
use horizon_core::geometry::Aabb;
use horizon_ecs_proto::bridge::{counts, host_read, ids::id_of};
use std::collections::HashSet;

impl EcsLevel {
    /// The entities of the world with a kind (`counts::count`); 0 when inert.
    pub fn entity_count(&self) -> usize {
        self.world().map_or(0, counts::count)
    }
    /// The entities with a kind as the spawning reads them: id, box, alive, shulker (`host_read::mob_boxes`); empty when inert.
    pub fn mob_boxes(&self) -> Vec<(u32, Aabb, bool, bool)> {
        let Some(w) = self.world() else { return Vec::new() };
        host_read::mob_boxes(w).into_iter().filter_map(|(e, b, alive, shulker)| Some((id_of(w, e)?, b, alive, shulker))).collect()
    }
    /// The live entities of kind `ty` that nothing rides and that ride nothing, with their boxes (`getEntitiesOfClass` not being ridden); empty when inert.
    pub fn unridden_of_type(&self, ty: &str) -> Vec<(u32, Aabb)> {
        let (Some(w), Some(b)) = (self.world(), self.ecs()) else { return Vec::new() };
        host_read::mob_boxes(w)
            .into_iter()
            .filter(|&(e, _, alive, _)| alive && host_read::species(w, e) == Some(ty) && b.host_riders(e).is_empty() && !b.is_passenger(e))
            .filter_map(|(e, bx, _, _)| Some((id_of(w, e)?, bx)))
            .collect()
    }
    /// The entities with a kind, per kind (`counts::count_by_kind`, a scan for the reports); empty when inert.
    pub fn count_by_kind(&self) -> Vec<(&'static str, usize)> {
        self.world().map_or_else(Vec::new, counts::count_by_kind)
    }
    /// Whether a live entity holds one of `uuids` (`counts::holds_any_uuid`); false when inert.
    pub fn holds_any_uuid(&self, uuids: &HashSet<[i32; 4]>) -> bool {
        self.world().is_some_and(|w| counts::holds_any_uuid(w, uuids))
    }
}
