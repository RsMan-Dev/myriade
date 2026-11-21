//! The blocks that sense entities on `EcsLevel`: the sensitive-state table, the touched-cell events and the zone queries a block's tick asks.
//! An inert level answers as an empty world.
use super::level::EcsLevel;
use horizon_core::geometry::Aabb;
use std::sync::Arc;

impl EcsLevel {
    /// Hands the ECS the table of the states that react to an entity inside (see `game::ecs::inside::inside_table`).
    pub fn set_inside_table(&mut self, table: Arc<[u64]>) {
        if let Some(ecs) = self.ecs_mut() {
            ecs.set_inside_table(table);
        }
    }

    /// Whether the table was handed over (false when inert).
    pub fn has_inside_table(&self) -> bool {
        self.ecs().is_some_and(|e| e.has_inside_table())
    }

    /// How many entities that trigger plates and tripwires meet `area`: the creatures only, or everything the ECS holds.
    pub fn count_triggers_in(&self, area: Aabb, mobs_only: bool) -> usize {
        self.ecs().map_or(0, |e| e.count_triggers_in(area, mobs_only))
    }

    /// The ids of the creatures whose box meets `area`.
    pub fn creatures_in(&self, area: Aabb) -> Vec<u32> {
        self.ecs().map_or_else(Vec::new, |e| e.creatures_in(area))
    }

    /// The boxes of the minecarts meeting `area`.
    pub fn cart_bounds_in(&self, area: Aabb) -> Vec<Aabb> {
        self.ecs().map_or_else(Vec::new, |e| e.cart_bounds_in(area))
    }

    /// The boxes of the fishing hooks meeting `area`, each with whether it triggers blocks.
    pub fn hooks_in(&self, area: Aabb) -> Vec<(Aabb, bool)> {
        self.ecs().map_or_else(Vec::new, |e| e.hooks_in(area))
    }

    /// How many arrows meet `area` (the jar's `AbstractArrow`: arrows, spectral arrows and tridents).
    pub fn count_arrows_in(&self, area: Aabb) -> usize {
        self.ecs().map_or(0, |e| e.count_arrows_in(area))
    }

    /// The item frames in the cell `pos` hanging on `face` (3D data value): (holds an item, rotation) of each.
    pub fn frames_at(&self, pos: [i32; 3], face: u8) -> Vec<(bool, u8)> {
        self.ecs().map_or_else(Vec::new, |e| e.frames_at(pos, face))
    }
}
