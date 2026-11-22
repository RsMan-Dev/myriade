//! The blocks that sense entities, through `EcsWorld`: the table of the sensitive states, the touched-cell events, and the zone queries a
//! block's tick asks (entities that trigger, cart boxes, hook boxes, the item frame of a cell, the arrows). All read the spatial index.
use super::host::EcsWorld;
use crate::bridge::ids;
use crate::inside::{self, Sensitive};
use horizon_core::geometry::Aabb;
use std::sync::Arc;

impl EcsWorld {
    /// Hands over the table of the states that react to an entity inside (a bit per state id), built once by the host.
    pub fn set_inside_table(&mut self, table: Arc<[u64]>) {
        self.world_mut().resource_mut::<Sensitive>().0 = Some(table);
    }

    /// Whether the table was handed over.
    pub fn has_inside_table(&self) -> bool {
        self.world().resource::<Sensitive>().0.is_some()
    }

    /// How many non-player entities that trigger blocks meet `area`: the creatures only, or with `mobs_only == false` everything.
    pub fn count_triggers_in(&self, area: Aabb, mobs_only: bool) -> usize {
        inside::triggers_in(self.world(), area, mobs_only).len()
    }

    /// The ids of the entities that trigger blocks in `area` (see `count_triggers_in`).
    pub fn triggers_in(&self, area: Aabb, mobs_only: bool) -> Vec<u32> {
        inside::triggers_in(self.world(), area, mobs_only).into_iter().filter_map(|e| ids::id_of(self.world(), e)).collect()
    }

    /// The linked ids of the creatures whose box meets `area` (spatial index, no scan).
    pub fn creatures_in(&self, area: Aabb) -> Vec<u32> {
        let w = self.world();
        crate::area::creatures_in(w, area, bevy_ecs::entity::Entity::PLACEHOLDER).into_iter().filter_map(|e| ids::id_of(w, e)).collect()
    }

    /// The boxes of the minecarts meeting `area`.
    pub fn cart_bounds_in(&self, area: Aabb) -> Vec<Aabb> {
        inside::cart_boxes_in(self.world(), area)
    }

    /// The boxes of the boats meeting `area`, by the spatial index.
    pub fn boat_bounds_in(&self, area: Aabb) -> Vec<Aabb> {
        inside::boat_boxes_in(self.world(), area)
    }

    /// The boxes of the fishing hooks meeting `area`, each with whether it triggers blocks.
    pub fn hooks_in(&self, area: Aabb) -> Vec<(Aabb, bool)> {
        inside::hooks_in(self.world(), area)
    }

    /// How many arrows (arrows, spectral arrows and tridents: `AbstractArrow`) meet `area`.
    pub fn count_arrows_in(&self, area: Aabb) -> usize {
        inside::arrows_in(self.world(), area).len()
    }

    /// The item frames in the cell `pos` hanging on `face` (`Direction.get3DDataValue`): (holds an item, rotation) of each.
    pub fn frames_at(&self, pos: [i32; 3], face: u8) -> Vec<(bool, u8)> {
        inside::frames_at(self.world(), pos, face)
    }
}
