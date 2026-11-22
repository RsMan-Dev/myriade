//! Reads by network id of an entity's body and identity: the id is turned into its entity by the level's table, then read from the
//! world (`host_read`). An absent or sleeping entity, and an inert level, read as None or the neutral value.
use super::level::EcsLevel;
use bevy_ecs::{entity::Entity, world::World};
use horizon_core::geometry::{Aabb, Vec3};
use horizon_ecs_proto::bridge::{
    host::Select,
    host_read::{self, Motion},
    select,
};

impl EcsLevel {
    /// The world and the entity of the id `id`, when the level is live and knows it.
    fn at(&self, id: u32) -> Option<(&World, Entity)> {
        Some((self.world()?, self.entity_of(id)?))
    }
    pub fn bounds(&self, id: u32) -> Option<Aabb> {
        let (w, e) = self.at(id)?;
        host_read::bounds(w, e)
    }
    /// The bounds of the entity `e` itself (a result of `select`, not yet linked to a network id).
    pub fn bounds_of(&self, e: Entity) -> Option<Aabb> {
        host_read::bounds(self.world()?, e)
    }
    pub fn alive(&self, id: u32) -> bool {
        self.at(id).is_some_and(|(w, e)| host_read::alive(w, e))
    }
    pub fn motion(&self, id: u32) -> Option<Motion> {
        let (w, e) = self.at(id)?;
        host_read::motion(w, e)
    }
    pub fn velocity(&self, id: u32) -> Option<Vec3> {
        let (w, e) = self.at(id)?;
        host_read::velocity(w, e)
    }
    pub fn yaw(&self, id: u32) -> Option<f32> {
        let (w, e) = self.at(id)?;
        host_read::yaw(w, e)
    }
    /// The head yaw and the body yaw of the creature `id` (`Head`); None without a head.
    pub fn head_body_yaw(&self, id: u32) -> Option<(f32, f32)> {
        let (w, e) = self.at(id)?;
        host_read::head_body_yaw(w, e)
    }
    pub fn eye_offset(&self, id: u32) -> Option<f64> {
        let (w, e) = self.at(id)?;
        host_read::eye_offset(w, e)
    }
    pub fn height(&self, id: u32) -> Option<f32> {
        let (w, e) = self.at(id)?;
        host_read::height(w, e)
    }
    pub fn pushable(&self, id: u32) -> bool {
        self.at(id).is_some_and(|(w, e)| host_read::pushable(w, e))
    }
    pub fn health(&self, id: u32) -> Option<f32> {
        let (w, e) = self.at(id)?;
        host_read::health(w, e)
    }
    pub fn max_health(&self, id: u32) -> Option<f32> {
        let (w, e) = self.at(id)?;
        host_read::max_health(w, e)
    }
    pub fn fall_distance(&self, id: u32) -> f64 {
        self.at(id).map_or(0.0, |(w, e)| host_read::fall_distance(w, e))
    }
    pub fn has_tag(&self, id: u32, tag: &str) -> bool {
        self.at(id).is_some_and(|(w, e)| host_read::has_tag(w, e, tag))
    }
    /// The network id of the entity `e` (`EcsWorld::id_of`); None when it has none or the level is inert.
    pub fn id_of(&self, e: Entity) -> Option<u32> {
        self.ecs()?.id_of(e)
    }
    /// A mob is an entity that carries a species (`TypeName`), as the mods' list of mobs reads them.
    pub fn is_mob(&self, id: u32) -> bool {
        self.species(id).is_some()
    }
    /// `host_read::sits_on_chest`.
    pub fn sits_on_chest(&self, id: u32) -> bool {
        self.at(id).is_some_and(|(w, e)| host_read::sits_on_chest(w, e))
    }
    pub fn species(&self, id: u32) -> Option<&'static str> {
        let (w, e) = self.at(id)?;
        host_read::species(w, e)
    }
    pub fn species_count(&self, kind: &str) -> usize {
        self.world().map_or(0, |w| host_read::species_count(w, kind))
    }
    /// The boxes of the awake entities of the species `kind`.
    pub fn species_bounds(&self, kind: &str) -> Vec<Aabb> {
        let q = Select { kind: Some(kind.to_owned()), tag: None, near: None, limit: usize::MAX };
        self.select(&q).into_iter().filter_map(|e| self.bounds_of(e)).collect()
    }
    /// The network ids of the awake entities of the species `kind`, in entity order.
    pub fn species_ids(&self, kind: &str) -> Vec<u32> {
        let q = Select { kind: Some(kind.to_owned()), tag: None, near: None, limit: usize::MAX };
        self.select(&q).into_iter().filter_map(|e| self.id_of(e)).collect()
    }
    /// The awake entities that match `q`, counted (`select::count`).
    pub fn count_entities(&self, q: &Select) -> usize {
        self.world().map_or(0, |w| select::count(w, q))
    }
}
