use super::*;
pub(super) struct PreparedWorldReload {
    pub(super) structure_spawns: myriade_engine::mobs::spawn_place::StructureSpawns,
    pub(super) advancements: Arc<myriade_engine::advancements::Tree>,
    pub(super) tables: Option<WorldTables>,
    pub(super) health: myriade_engine::health::HealthResources,
    pub(super) wire: Option<Arc<myriade_engine::lan::data::SyncData>>,
}
