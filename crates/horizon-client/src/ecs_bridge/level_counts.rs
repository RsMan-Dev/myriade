//! The counts of the level's ECS world, for the gauges, the reports and the dimension-transfer check: each is one read of
//! `bridge::counts`. An inert level counts nothing.
use super::level::EcsLevel;
use horizon_ecs_proto::bridge::counts;
use std::collections::HashSet;

impl EcsLevel {
    /// The entities of the world with a kind (`counts::count`); 0 when inert.
    pub fn entity_count(&self) -> usize {
        self.world().map_or(0, counts::count)
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
