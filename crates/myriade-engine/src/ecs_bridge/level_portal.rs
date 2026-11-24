//! The portals of the ECS entities by network id (lot 73): the death a host forces, the entries of the portals and the box. An inert level answers as an empty world: none, false, nothing.
use super::level::EcsLevel;
use myriade_engine_lib::mobs::MobWorld;
use horizon_ecs_proto::bridge::ids::id_of;

impl EcsLevel {
    /// `Entity.kill` of the entity `id`; the item names it dropped (`EcsWorld::kill`).
    pub fn kill(&mut self, mw: &MobWorld<'_>, id: u32) -> Vec<&'static str> {
        let Some(e) = self.entity_of(id) else { return Vec::new() };
        self.ecs_mut().map_or_else(Vec::new, |b| b.kill(mw, e))
    }
    /// The width and height of the entity `id` (`EcsWorld::size`, read by lot 40).
    pub fn size(&self, id: u32) -> Option<(f32, f32)> {
        self.ecs()?.size(self.entity_of(id)?)
    }
    /// The entities that moved, as (id, may take a portal with its vehicle, where it is): `EcsWorld::portal_entries`.
    pub fn portal_entries(&mut self, mw: &MobWorld<'_>) -> Vec<(u32, bool, [f64; 3])> {
        let Some(b) = self.ecs_mut() else { return Vec::new() };
        let rows = b.portal_entries(mw);
        let Some(w) = self.world() else { return Vec::new() };
        rows.into_iter().filter_map(|(e, usable, to)| Some((id_of(w, e)?, usable, to))).collect()
    }
    /// Whether any entity moved (`EcsWorld::any_moving`): read before a mob world is built for `portal_entries`.
    pub fn any_moving(&self) -> bool {
        self.ecs().is_some_and(|b| b.any_moving())
    }
    /// `EcsWorld::can_use_portal` of the entity `id`.
    pub fn can_use_portal(&self, id: u32) -> bool {
        self.entity_of(id).zip(self.ecs()).is_some_and(|(e, b)| b.can_use_portal(e))
    }
    /// `Entity.portal_after_move` of the entity `id` (`EcsWorld::portal_after_move`); false when it is absent.
    pub fn portal_after_move(&self, id: u32) -> bool {
        self.entity_of(id).zip(self.ecs()).is_some_and(|(e, b)| b.portal_after_move(e))
    }
    /// The body yaw and head pitch of the entity `id` (`EcsWorld::rotation`).
    pub fn rotation(&self, id: u32) -> Option<[f32; 2]> {
        self.ecs()?.rotation(self.entity_of(id)?)
    }
    /// The box of the entity `id` (`EcsWorld::portal_size`).
    pub fn portal_size(&self, id: u32) -> Option<(f32, f32)> {
        self.ecs()?.portal_size(self.entity_of(id)?)
    }
}
